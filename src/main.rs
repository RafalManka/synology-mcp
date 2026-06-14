use anyhow::{Context, Error};
use axum::{Router, middleware};
use std::sync::Arc;
use tracing::info;

mod auth;
mod dsm;
mod server;

use crate::auth::auth_middleware;
use dsm::client::DsmClient;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::never::NeverSessionManager,
};
use server::SynologyMcp;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    setup();
    serve(build_router().await?, &build_address()?).await?;
    Ok(())
}

fn setup() {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "synology_mcp=info".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();
}

async fn serve(app: Router, addr: &str) -> Result<(), Error> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind to {addr} — port already in use?"))?;

    info!("synology-mcp listening on http://{addr}/mcp");

    axum::serve(listener, app).await?;
    Ok(())
}

async fn build_router() -> Result<Router, Error> {
    let dsm = build_dsm_client().await?;
    let service = SynologyMcp::new(dsm);

    let config = {
        let mut c = StreamableHttpServerConfig::default();
        c.stateful_mode = false;
        c.allowed_hosts.clear();
        c
    };
    let mcp_service = StreamableHttpService::new(
        move || Ok(service.clone()),
        Arc::new(NeverSessionManager::default()),
        config,
    );
    let auth_token = std::env::var("MCP_AUTH_TOKEN").context("MCP_AUTH_TOKEN required")?;
    let app = Router::new()
        .route_service("/mcp", mcp_service)
        .layer(middleware::from_fn_with_state(auth_token, auth_middleware));
    Ok(app)
}

fn build_address() -> Result<String, Error> {
    let mcp_port: u16 = std::env::var("MCP_PORT")
        .unwrap_or_else(|_| "3000".into())
        .parse()
        .context("MCP_PORT must be a number")?;
    let addr = format!("0.0.0.0:{mcp_port}");
    Ok(addr)
}

async fn build_dsm_client() -> Result<Arc<DsmClient>, Error> {
    let host = std::env::var("SYNOLOGY_HOST").context("SYNOLOGY_HOST required")?;
    let port: u16 = std::env::var("SYNOLOGY_PORT")
        .unwrap_or_else(|_| "5001".into())
        .parse()
        .context("SYNOLOGY_PORT must be a number")?;
    let user = std::env::var("SYNOLOGY_USER").context("SYNOLOGY_USER required")?;
    let password = std::env::var("SYNOLOGY_PASSWORD").context("SYNOLOGY_PASSWORD required")?;
    let https = std::env::var("SYNOLOGY_HTTPS")
        .unwrap_or_else(|_| "true".into())
        .to_lowercase()
        != "false";

    let dsm = Arc::new(
        DsmClient::new(host, port, https, user, password)
            .await
            .context("Failed to connect to DSM")?,
    );
    Ok(dsm)
}

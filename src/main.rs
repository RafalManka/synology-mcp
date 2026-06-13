use anyhow::Context;
use axum::{Router, extract::Request, http::StatusCode, middleware::{self, Next}, response::IntoResponse};
use std::sync::Arc;
use tracing::info;

mod dsm;
mod server;

use dsm::client::DsmClient;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService,
    session::never::NeverSessionManager,
};
use server::SynologyMcp;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env if present (local dev only — no-op in production)
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "synology_mcp=info".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();

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

    let auth_token = std::env::var("MCP_AUTH_TOKEN").context("MCP_AUTH_TOKEN required")?;
    let mcp_port: u16 = std::env::var("MCP_PORT")
        .unwrap_or_else(|_| "3000".into())
        .parse()
        .context("MCP_PORT must be a number")?;

    let dsm = Arc::new(
        DsmClient::new(host, port, https, user, password)
            .await
            .context("Failed to connect to DSM")?,
    );

    let service = SynologyMcp::new(dsm);

    // Stateless mode — each request is independent, no session continuity needed
    // This is correct for tool-based MCP servers where each call is self-contained
    let config = {
        let mut c = StreamableHttpServerConfig::default();
        c.stateful_mode = false;
        c.allowed_hosts.clear(); // disable host check — we're behind a reverse proxy
        c
    };

    let mcp_service = StreamableHttpService::new(
        move || Ok(service.clone()),
        Arc::new(NeverSessionManager::default()),
        config,
    );

    // Bearer token auth middleware
    let token = auth_token.clone();
    let app = Router::new()
        .route_service("/mcp", mcp_service)
        .layer(middleware::from_fn(move |req: Request, next: Next| {
            let token = token.clone();
            async move {
                let authorized = req
                    .headers()
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.strip_prefix("Bearer "))
                    .is_some_and(|t| t == token);

                if authorized {
                    next.run(req).await
                } else {
                    StatusCode::UNAUTHORIZED.into_response()
                }
            }
        }));

    let addr = format!("0.0.0.0:{mcp_port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr} — port already in use?"))?;

    info!("synology-mcp listening on http://{addr}/mcp");

    axum::serve(listener, app).await?;

    Ok(())
}

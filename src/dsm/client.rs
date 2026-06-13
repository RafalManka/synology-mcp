use anyhow::{anyhow, Context};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

pub struct DsmClient {
    client: reqwest::Client,
    base_url: String,
    sid: Arc<RwLock<String>>,
}

impl DsmClient {
    pub async fn new(
        host: String,
        port: u16,
        https: bool,
        user: String,
        password: String,
    ) -> anyhow::Result<Self> {
        let scheme = if https { "https" } else { "http" };
        let base_url = format!("{scheme}://{host}:{port}");

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true) // self-signed cert on most home NAS
            .build()
            .context("failed to build HTTP client")?;

        let sid = Self::login(&client, &base_url, &user, &password).await?;
        tracing::info!("DSM session established");

        Ok(Self {
            client,
            base_url,
            sid: Arc::new(RwLock::new(sid)),
        })
    }

    async fn login(
        client: &reqwest::Client,
        base_url: &str,
        user: &str,
        password: &str,
    ) -> anyhow::Result<String> {
        debug!("Logging in to DSM as {user}");

        let url = format!("{base_url}/webapi/entry.cgi");
        let resp = client
            .post(&url)
            .form(&[
                ("api", "SYNO.API.Auth"),
                ("version", "7"),
                ("method", "login"),
                ("account", user),
                ("passwd", password),
                ("session", "SynologyMCP"),
                ("format", "sid"),
            ])
            .send()
            .await
            .context("DSM login request failed")?;

        let body: Value = resp.json().await.context("DSM login response not JSON")?;

        if body["success"].as_bool() != Some(true) {
            let code = body["error"]["code"].as_u64().unwrap_or(0);
            return Err(anyhow!("DSM login failed (error code {code})"));
        }

        body["data"]["sid"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("DSM login response missing sid"))
    }

    pub async fn call(
        &self,
        api: &str,
        version: u32,
        method: &str,
        extra: &[(&str, &str)],
    ) -> anyhow::Result<Value> {
        let sid = self.sid.read().await.clone();
        let url = format!("{}/webapi/entry.cgi", self.base_url);

        debug!("DSM call: api={api} method={method}");

        let mut params = vec![
            ("api", api.to_string()),
            ("version", version.to_string()),
            ("method", method.to_string()),
            ("_sid", sid),
        ];
        for (k, v) in extra {
            params.push((k, v.to_string()));
        }

        let resp = self
            .client
            .post(&url)
            .form(&params)
            .send()
            .await
            .with_context(|| format!("DSM request failed: {api}.{method}"))?;

        let body: Value = resp
            .json()
            .await
            .context("DSM response not valid JSON")?;

        if body["success"].as_bool() != Some(true) {
            let code = body["error"]["code"].as_u64().unwrap_or(0);
            return Err(anyhow!("DSM API error: {api}.{method} code={code}"));
        }

        Ok(body["data"].clone())
    }
}

impl Drop for DsmClient {
    fn drop(&mut self) {
        // best-effort logout — ignore errors
        let client = self.client.clone();
        let base_url = self.base_url.clone();
        let sid = self.sid.clone();
        tokio::spawn(async move {
            let sid = sid.read().await.clone();
            let url = format!("{base_url}/webapi/entry.cgi");
            let result = client
                .post(&url)
                .form(&[
                    ("api", "SYNO.API.Auth"),
                    ("version", "7"),
                    ("method", "logout"),
                    ("session", "SynologyMCP"),
                    ("_sid", &sid),
                ])
                .send()
                .await;
            if let Err(e) = result {
                warn!("DSM logout failed: {e}");
            }
        });
    }
}

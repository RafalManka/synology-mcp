use std::sync::Arc;

use rmcp::{
    ServerHandler, tool_handler,
    handler::server::router::tool::ToolRouter,
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool, tool_router,
    ErrorData as McpError,
};

use crate::dsm::{client::DsmClient, storage, system};

#[derive(Clone)]
pub struct SynologyMcp {
    dsm: Arc<DsmClient>,
    tool_router: ToolRouter<Self>,
}

impl SynologyMcp {
    pub fn new(dsm: Arc<DsmClient>) -> Self {
        Self {
            dsm,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl SynologyMcp {
    #[tool(description = "Get Synology NAS system information: model, serial, DSM version, uptime, hostname, temperature, RAM")]
    async fn get_system_info(&self) -> Result<String, McpError> {
        let info = system::get_system_info(&self.dsm)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        serde_json::to_string_pretty(&info)
            .map_err(|e| McpError::internal_error(e.to_string(), None))
    }

    #[tool(description = "Get real-time CPU usage, memory usage, and network throughput of the Synology NAS")]
    async fn get_system_utilisation(&self) -> Result<String, McpError> {
        let util = system::get_system_utilisation(&self.dsm)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        serde_json::to_string_pretty(&util)
            .map_err(|e| McpError::internal_error(e.to_string(), None))
    }

    #[tool(description = "List all installed DSM packages with name, version, and running status")]
    async fn list_packages(&self) -> Result<String, McpError> {
        let packages = system::list_packages(&self.dsm)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        serde_json::to_string_pretty(&packages)
            .map_err(|e| McpError::internal_error(e.to_string(), None))
    }

    #[tool(description = "Get storage volume information: total/used/free space, status, filesystem type for each volume")]
    async fn get_volumes(&self) -> Result<String, McpError> {
        let volumes = storage::get_volumes(&self.dsm)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        serde_json::to_string_pretty(&volumes)
            .map_err(|e| McpError::internal_error(e.to_string(), None))
    }

    #[tool(description = "Get disk information: model, serial, temperature, health status, SMART status, size for each drive")]
    async fn get_disks(&self) -> Result<String, McpError> {
        let disks = storage::get_disks(&self.dsm)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        serde_json::to_string_pretty(&disks)
            .map_err(|e| McpError::internal_error(e.to_string(), None))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SynologyMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("synology-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Synology NAS management tools. Provides system info, storage health, \
                 disk SMART status, and installed packages.",
            )
    }
}

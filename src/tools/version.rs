use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::*,
    tool, tool_handler, tool_router,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD_SHA: &str = match option_env!("BUILD_SHA") {
    Some(sha) => sha,
    None => "dev",
};

#[derive(Clone)]
pub struct GoldentoothMcp {
    tool_router: ToolRouter<GoldentoothMcp>,
}

#[tool_router]
impl GoldentoothMcp {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Get the MCP server version, build info, and server name")]
    fn get_version(&self) -> Result<CallToolResult, McpError> {
        let info = serde_json::json!({
            "version": VERSION,
            "build": BUILD_SHA,
            "server": "goldentooth-mcp",
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&info).unwrap(),
        )]))
    }

    #[tool(description = "List all Raspberry Pi nodes in the bramble cluster with their hardware info")]
    fn list_nodes(&self) -> Result<CallToolResult, McpError> {
        let nodes = serde_json::json!({
            "cluster": "goldentooth",
            "nodes": [
                {"name": "allyrion",  "model": "Pi 4B", "role": "worker"},
                {"name": "bettley",   "model": "Pi 4B", "role": "worker"},
                {"name": "cargyll",   "model": "Pi 4B", "role": "worker"},
                {"name": "dalt",      "model": "Pi 4B", "role": "worker"},
                {"name": "erenford",  "model": "Pi 4B", "role": "worker"},
                {"name": "fenn",      "model": "Pi 4B", "role": "worker"},
                {"name": "gardener",  "model": "Pi 4B", "role": "worker"},
                {"name": "harlton",   "model": "Pi 4B", "role": "worker"},
                {"name": "inchfield", "model": "Pi 4B", "role": "worker"},
                {"name": "jast",      "model": "Pi 4B", "role": "worker"},
                {"name": "karstark",  "model": "Pi 4B", "role": "worker"},
                {"name": "lipps",     "model": "Pi 4B", "role": "worker"},
                {"name": "manderly",  "model": "Pi 5",  "role": "worker"},
                {"name": "norcross",  "model": "Pi 5",  "role": "worker"},
                {"name": "oakheart",  "model": "Pi 5",  "role": "worker"},
                {"name": "payne",     "model": "Pi 5",  "role": "worker"},
            ],
            "total": 16,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&nodes).unwrap(),
        )]))
    }
}

#[tool_handler]
impl ServerHandler for GoldentoothMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        .with_server_info(Implementation::new("goldentooth-mcp", VERSION))
        .with_instructions(
            "Goldentooth MCP server for managing a Raspberry Pi bramble cluster.".to_string(),
        )
    }
}

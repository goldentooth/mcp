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

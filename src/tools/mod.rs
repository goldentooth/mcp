pub mod cluster;
pub mod flux;
pub mod observability;
pub mod version;

use rmcp::{ErrorData as McpError, model::ErrorCode};

/// Convert any error into an MCP tool error.
pub fn tool_error(e: impl std::fmt::Display) -> McpError {
    McpError::new(
        ErrorCode::INTERNAL_ERROR,
        format!("{e}"),
        None::<serde_json::Value>,
    )
}

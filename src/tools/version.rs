use kube::Client;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{wrapper::Parameters, router::tool::ToolRouter},
    model::*,
    tool, tool_handler, tool_router,
};

use super::cluster::{self, NamespaceFilter};
use super::observability::{self, LogQuery, MetricQuery};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD_SHA: &str = match option_env!("BUILD_SHA") {
    Some(sha) => sha,
    None => "dev",
};

#[derive(Clone)]
pub struct GoldentoothMcp {
    tool_router: ToolRouter<GoldentoothMcp>,
    kube_client: Option<Client>,
    http_client: reqwest::Client,
}

#[tool_router]
impl GoldentoothMcp {
    pub fn new(kube_client: Option<Client>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            kube_client,
            http_client: reqwest::Client::new(),
        }
    }

    fn require_kube(&self) -> Result<&Client, McpError> {
        self.kube_client.as_ref().ok_or_else(|| {
            McpError::new(
                ErrorCode::INTERNAL_ERROR,
                "Kubernetes client not available (not running in-cluster?)",
                None::<serde_json::Value>,
            )
        })
    }

    // ── Static tools ──────────────────────────────────────────────

    #[tool(description = "Get the MCP server version, build info, and server name")]
    fn get_version(&self) -> Result<CallToolResult, McpError> {
        let info = serde_json::json!({
            "version": VERSION,
            "build": BUILD_SHA,
            "server": "goldentooth-mcp",
            "kubernetes": self.kube_client.is_some(),
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&info).unwrap(),
        )]))
    }

    // ── Kubernetes cluster tools ──────────────────────────────────

    #[tool(description = "Get real-time status of all Kubernetes nodes in the bramble cluster, including readiness, CPU, memory, OS, and kubelet version")]
    async fn get_node_status(&self) -> Result<CallToolResult, McpError> {
        cluster::get_node_status(self.require_kube()?).await
    }

    #[tool(description = "List pods running in the cluster. Optionally filter by namespace.")]
    async fn get_pods(
        &self,
        Parameters(input): Parameters<NamespaceFilter>,
    ) -> Result<CallToolResult, McpError> {
        cluster::get_pods(self.require_kube()?, input.namespace.as_deref()).await
    }

    #[tool(description = "List all Kubernetes namespaces and their status")]
    async fn get_namespaces(&self) -> Result<CallToolResult, McpError> {
        cluster::get_namespaces(self.require_kube()?).await
    }

    #[tool(description = "Get recent Kubernetes events. Optionally filter by namespace. Returns the 50 most recent events sorted by time.")]
    async fn get_events(
        &self,
        Parameters(input): Parameters<NamespaceFilter>,
    ) -> Result<CallToolResult, McpError> {
        cluster::get_events(self.require_kube()?, input.namespace.as_deref()).await
    }

    #[tool(description = "Get workload summary (Deployments, StatefulSets, DaemonSets) with ready/desired replica counts. Optionally filter by namespace.")]
    async fn get_workloads(
        &self,
        Parameters(input): Parameters<NamespaceFilter>,
    ) -> Result<CallToolResult, McpError> {
        cluster::get_workloads(self.require_kube()?, input.namespace.as_deref()).await
    }

    // ── Observability tools ───────────────────────────────────────

    #[tool(description = "Get all cert-manager TLS certificates with their status, expiry time, renewal time, issuer, and DNS names")]
    async fn get_certificates(&self) -> Result<CallToolResult, McpError> {
        observability::get_certificates(self.require_kube()?).await
    }

    #[tool(description = "Get active alerts from Alertmanager with severity, status, and descriptions")]
    async fn get_alerts(&self) -> Result<CallToolResult, McpError> {
        observability::get_alerts(&self.http_client).await
    }

    #[tool(description = "Query logs from Loki using LogQL. Example queries: '{namespace=\"forgejo\"}', '{app=\"goldentooth-mcp\"} |= \"error\"', '{job=\"systemd-journal\"} |= \"OOM\"'")]
    async fn query_logs(
        &self,
        Parameters(input): Parameters<LogQuery>,
    ) -> Result<CallToolResult, McpError> {
        observability::query_logs(&self.http_client, &input.query, input.limit).await
    }

    #[tool(description = "Query Prometheus metrics using PromQL. Example queries: 'up', 'node_memory_MemAvailable_bytes', 'rate(container_cpu_usage_seconds_total[5m])', 'kube_node_status_condition{condition=\"Ready\",status=\"true\"}'")]
    async fn query_metrics(
        &self,
        Parameters(input): Parameters<MetricQuery>,
    ) -> Result<CallToolResult, McpError> {
        observability::query_metrics(&self.http_client, &input.query).await
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

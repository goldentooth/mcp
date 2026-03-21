use kube::api::ListParams;
use kube::Client;
use rmcp::{ErrorData as McpError, model::*};
use serde::Deserialize;

use super::tool_error;

// In-cluster service endpoints.
const ALERTMANAGER_URL: &str =
    "http://monitoring-kube-prometheus-alertmanager.monitoring.svc:9093";
const LOKI_URL: &str = "http://monitoring-loki.monitoring.svc.cluster.local:3100";
const PROMETHEUS_URL: &str =
    "http://monitoring-kube-prometheus-prometheus.monitoring.svc.cluster.local:9090";

/// Get all cert-manager Certificate resources with their status.
pub async fn get_certificates(client: &Client) -> Result<CallToolResult, McpError> {
    // Use the dynamic API since cert-manager CRDs aren't in k8s-openapi.
    let certs_api = kube::Api::<kube::api::DynamicObject>::all_with(
        client.clone(),
        &kube::discovery::ApiResource {
            group: "cert-manager.io".into(),
            version: "v1".into(),
            api_version: "cert-manager.io/v1".into(),
            kind: "Certificate".into(),
            plural: "certificates".into(),
        },
    );

    let certs = certs_api
        .list(&ListParams::default())
        .await
        .map_err(tool_error)?;

    let cert_info: Vec<serde_json::Value> = certs
        .items
        .iter()
        .map(|cert| {
            let name = cert.metadata.name.as_deref().unwrap_or("unknown");
            let ns = cert.metadata.namespace.as_deref().unwrap_or("unknown");
            let data = &cert.data;

            let ready = data
                .pointer("/status/conditions")
                .and_then(|c| c.as_array())
                .and_then(|conditions| {
                    conditions
                        .iter()
                        .find(|c| c.get("type").and_then(|t| t.as_str()) == Some("Ready"))
                })
                .and_then(|c| c.get("status"))
                .and_then(|s| s.as_str())
                .unwrap_or("Unknown");

            let not_after = data
                .pointer("/status/notAfter")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let renewal_time = data
                .pointer("/status/renewalTime")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let secret_name = data
                .pointer("/spec/secretName")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let issuer = data
                .pointer("/spec/issuerRef/name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let dns_names: Vec<&str> = data
                .pointer("/spec/dnsNames")
                .and_then(|v| v.as_array())
                .map(|names| {
                    names
                        .iter()
                        .filter_map(|n| n.as_str())
                        .collect()
                })
                .unwrap_or_default();

            serde_json::json!({
                "name": name,
                "namespace": ns,
                "ready": ready,
                "expires": not_after,
                "renewal": renewal_time,
                "secret": secret_name,
                "issuer": issuer,
                "dns_names": dns_names,
            })
        })
        .collect();

    let result = serde_json::json!({
        "certificates": cert_info,
        "total": cert_info.len(),
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&result).unwrap(),
    )]))
}

/// Get active alerts from Alertmanager.
pub async fn get_alerts(http: &reqwest::Client) -> Result<CallToolResult, McpError> {
    let url = format!("{ALERTMANAGER_URL}/api/v2/alerts");
    let resp: Vec<serde_json::Value> = http
        .get(&url)
        .send()
        .await
        .map_err(tool_error)?
        .json()
        .await
        .map_err(tool_error)?;

    let alerts: Vec<serde_json::Value> = resp
        .iter()
        .map(|alert| {
            let labels = alert.get("labels").cloned().unwrap_or_default();
            let annotations = alert.get("annotations").cloned().unwrap_or_default();
            let status = alert
                .pointer("/status/state")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let starts_at = alert
                .get("startsAt")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");

            serde_json::json!({
                "alertname": labels.get("alertname").and_then(|v| v.as_str()).unwrap_or("unknown"),
                "severity": labels.get("severity").and_then(|v| v.as_str()).unwrap_or("unknown"),
                "status": status,
                "starts_at": starts_at,
                "summary": annotations.get("summary").and_then(|v| v.as_str()).unwrap_or(""),
                "description": annotations.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                "labels": labels,
            })
        })
        .collect();

    let result = serde_json::json!({
        "alerts": alerts,
        "total": alerts.len(),
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&result).unwrap(),
    )]))
}

/// Query Loki logs using LogQL.
pub async fn query_logs(
    http: &reqwest::Client,
    query: &str,
    limit: Option<u32>,
) -> Result<CallToolResult, McpError> {
    let limit = limit.unwrap_or(100).min(500);
    let url = format!("{LOKI_URL}/loki/api/v1/query_range");

    let resp: serde_json::Value = http
        .get(&url)
        .query(&[
            ("query", query),
            ("limit", &limit.to_string()),
        ])
        .send()
        .await
        .map_err(tool_error)?
        .json()
        .await
        .map_err(tool_error)?;

    // Extract log lines from the Loki response.
    let status = resp
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown");

    let streams = resp
        .pointer("/data/result")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    let mut lines: Vec<serde_json::Value> = Vec::new();
    for stream in &streams {
        let labels = stream.get("stream").cloned().unwrap_or_default();
        if let Some(values) = stream.get("values").and_then(|v| v.as_array()) {
            for entry in values {
                if let Some(arr) = entry.as_array() {
                    let ts = arr.first().and_then(|v| v.as_str()).unwrap_or("");
                    let line = arr.get(1).and_then(|v| v.as_str()).unwrap_or("");
                    lines.push(serde_json::json!({
                        "timestamp": ts,
                        "line": line,
                        "labels": labels,
                    }));
                }
            }
        }
    }

    let result = serde_json::json!({
        "status": status,
        "lines": lines,
        "total": lines.len(),
        "query": query,
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&result).unwrap(),
    )]))
}

/// Query Prometheus metrics using PromQL.
pub async fn query_metrics(
    http: &reqwest::Client,
    query: &str,
) -> Result<CallToolResult, McpError> {
    let url = format!("{PROMETHEUS_URL}/api/v1/query");

    let resp: serde_json::Value = http
        .get(&url)
        .query(&[("query", query)])
        .send()
        .await
        .map_err(tool_error)?
        .json()
        .await
        .map_err(tool_error)?;

    let status = resp
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown");

    let result_type = resp
        .pointer("/data/resultType")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown");

    let results = resp
        .pointer("/data/result")
        .cloned()
        .unwrap_or(serde_json::json!([]));

    let result = serde_json::json!({
        "status": status,
        "result_type": result_type,
        "results": results,
        "query": query,
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&result).unwrap(),
    )]))
}

/// Input for log queries.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LogQuery {
    /// LogQL query string, e.g. '{namespace="forgejo"}' or '{app="goldentooth-mcp"} |= "error"'
    pub query: String,
    /// Maximum number of log lines to return (default 100, max 500).
    pub limit: Option<u32>,
}

/// Input for metric queries.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MetricQuery {
    /// PromQL query string, e.g. 'up', 'node_memory_MemAvailable_bytes', 'rate(container_cpu_usage_seconds_total[5m])'
    pub query: String,
}

// In-cluster ntfy endpoint.
const NTFY_URL: &str = "http://ntfy.ntfy.svc:80";

/// Get recent ntfy notifications from the cluster-alerts topic.
pub async fn get_notifications(
    http: &reqwest::Client,
    topic: &str,
    since: Option<&str>,
) -> Result<CallToolResult, McpError> {
    let since = since.unwrap_or("24h");
    let url = format!("{NTFY_URL}/{topic}/json");

    let resp = http
        .get(&url)
        .query(&[("poll", "1"), ("since", since)])
        .send()
        .await
        .map_err(tool_error)?
        .text()
        .await
        .map_err(tool_error)?;

    // ntfy returns newline-delimited JSON, one message per line.
    let messages: Vec<serde_json::Value> = resp
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .filter(|msg: &serde_json::Value| {
            msg.get("event").and_then(|e| e.as_str()) == Some("message")
        })
        .map(|msg| {
            serde_json::json!({
                "title": msg.get("title").and_then(|t| t.as_str()).unwrap_or(""),
                "message": msg.get("message").and_then(|m| m.as_str()).unwrap_or(""),
                "priority": msg.get("priority").and_then(|p| p.as_i64()).unwrap_or(3),
                "tags": msg.get("tags").cloned().unwrap_or(serde_json::json!([])),
                "time": msg.get("time").and_then(|t| t.as_i64()).unwrap_or(0),
                "topic": msg.get("topic").and_then(|t| t.as_str()).unwrap_or(""),
            })
        })
        .collect();

    let result = serde_json::json!({
        "notifications": messages,
        "total": messages.len(),
        "topic": topic,
        "since": since,
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&result).unwrap(),
    )]))
}

/// Input for notification queries.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NotificationQuery {
    /// ntfy topic to query (e.g. 'cluster-alerts').
    pub topic: String,
    /// How far back to look for notifications. Accepts durations like '1h', '24h', '7d' or Unix timestamps. Default: '24h'.
    pub since: Option<String>,
}

use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::core::v1::{Event, Namespace, Node, Pod};
use kube::api::ListParams;
use kube::{Api, Client};
use rmcp::{ErrorData as McpError, model::*};
use serde::Deserialize;

use super::tool_error;

/// Get real node status from the Kubernetes API.
pub async fn get_node_status(client: &Client) -> Result<CallToolResult, McpError> {
    let nodes: Api<Node> = Api::all(client.clone());
    let node_list = nodes.list(&ListParams::default()).await.map_err(tool_error)?;

    let node_info: Vec<serde_json::Value> = node_list
        .items
        .iter()
        .map(|node| {
            let name = node.metadata.name.as_deref().unwrap_or("unknown");
            let status = node.status.as_ref();

            let ready = status
                .and_then(|s| s.conditions.as_ref())
                .and_then(|conditions| conditions.iter().find(|c| c.type_ == "Ready"))
                .map(|c| c.status.as_str())
                .unwrap_or("Unknown");

            let cpu_capacity = status
                .and_then(|s| s.capacity.as_ref())
                .and_then(|c| c.get("cpu"))
                .map(|q| q.0.clone())
                .unwrap_or_default();

            let memory_capacity = status
                .and_then(|s| s.capacity.as_ref())
                .and_then(|c| c.get("memory"))
                .map(|q| q.0.clone())
                .unwrap_or_default();

            let os_image = status
                .and_then(|s| s.node_info.as_ref())
                .map(|i| i.os_image.as_str())
                .unwrap_or("unknown");

            let kubelet_version = status
                .and_then(|s| s.node_info.as_ref())
                .map(|i| i.kubelet_version.as_str())
                .unwrap_or("unknown");

            let arch = status
                .and_then(|s| s.node_info.as_ref())
                .map(|i| i.architecture.as_str())
                .unwrap_or("unknown");

            serde_json::json!({
                "name": name,
                "ready": ready,
                "cpu": cpu_capacity,
                "memory": memory_capacity,
                "os": os_image,
                "kubelet": kubelet_version,
                "arch": arch,
            })
        })
        .collect();

    let result = serde_json::json!({
        "nodes": node_info,
        "total": node_info.len(),
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&result).unwrap(),
    )]))
}

/// List pods, optionally filtered by namespace.
pub async fn get_pods(
    client: &Client,
    namespace: Option<&str>,
) -> Result<CallToolResult, McpError> {
    let pod_list = match namespace {
        Some(ns) => {
            let pods: Api<Pod> = Api::namespaced(client.clone(), ns);
            pods.list(&ListParams::default()).await.map_err(tool_error)?
        }
        None => {
            let pods: Api<Pod> = Api::all(client.clone());
            pods.list(&ListParams::default()).await.map_err(tool_error)?
        }
    };

    let pod_info: Vec<serde_json::Value> = pod_list
        .items
        .iter()
        .map(|pod| {
            let name = pod.metadata.name.as_deref().unwrap_or("unknown");
            let ns = pod.metadata.namespace.as_deref().unwrap_or("unknown");
            let phase = pod
                .status
                .as_ref()
                .and_then(|s| s.phase.as_deref())
                .unwrap_or("Unknown");
            let node = pod
                .spec
                .as_ref()
                .and_then(|s| s.node_name.as_deref())
                .unwrap_or("unscheduled");

            let containers: Vec<serde_json::Value> = pod
                .spec
                .as_ref()
                .map(|s| {
                    s.containers
                        .iter()
                        .map(|c| {
                            serde_json::json!({
                                "name": c.name,
                                "image": c.image.as_deref().unwrap_or("unknown"),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            let restarts: i32 = pod
                .status
                .as_ref()
                .and_then(|s| s.container_statuses.as_ref())
                .map(|statuses| statuses.iter().map(|cs| cs.restart_count).sum())
                .unwrap_or(0);

            serde_json::json!({
                "name": name,
                "namespace": ns,
                "phase": phase,
                "node": node,
                "restarts": restarts,
                "containers": containers,
            })
        })
        .collect();

    let result = serde_json::json!({
        "pods": pod_info,
        "total": pod_info.len(),
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&result).unwrap(),
    )]))
}

/// List all namespaces with their status.
pub async fn get_namespaces(client: &Client) -> Result<CallToolResult, McpError> {
    let namespaces: Api<Namespace> = Api::all(client.clone());
    let ns_list = namespaces
        .list(&ListParams::default())
        .await
        .map_err(tool_error)?;

    let ns_info: Vec<serde_json::Value> = ns_list
        .items
        .iter()
        .map(|ns| {
            let name = ns.metadata.name.as_deref().unwrap_or("unknown");
            let phase = ns
                .status
                .as_ref()
                .and_then(|s| s.phase.as_deref())
                .unwrap_or("Unknown");
            serde_json::json!({
                "name": name,
                "phase": phase,
            })
        })
        .collect();

    let result = serde_json::json!({
        "namespaces": ns_info,
        "total": ns_info.len(),
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&result).unwrap(),
    )]))
}

/// Get recent events, optionally filtered by namespace.
pub async fn get_events(
    client: &Client,
    namespace: Option<&str>,
) -> Result<CallToolResult, McpError> {
    let event_list = match namespace {
        Some(ns) => {
            let events: Api<Event> = Api::namespaced(client.clone(), ns);
            events
                .list(&ListParams::default())
                .await
                .map_err(tool_error)?
        }
        None => {
            let events: Api<Event> = Api::all(client.clone());
            events
                .list(&ListParams::default())
                .await
                .map_err(tool_error)?
        }
    };

    // Take the most recent 50 events.
    let mut events: Vec<&Event> = event_list.items.iter().collect();
    events.sort_by(|a, b| {
        let a_time = a.last_timestamp.as_ref().map(|t| &t.0);
        let b_time = b.last_timestamp.as_ref().map(|t| &t.0);
        b_time.cmp(&a_time)
    });
    events.truncate(50);

    let event_info: Vec<serde_json::Value> = events
        .iter()
        .map(|event| {
            let ns = event.metadata.namespace.as_deref().unwrap_or("unknown");
            let reason = event.reason.as_deref().unwrap_or("unknown");
            let message = event.message.as_deref().unwrap_or("");
            let kind = event
                .involved_object
                .kind
                .as_deref()
                .unwrap_or("unknown");
            let obj_name = event
                .involved_object
                .name
                .as_deref()
                .unwrap_or("unknown");
            let type_ = event.type_.as_deref().unwrap_or("Normal");
            let count = event.count.unwrap_or(1);
            let last = event
                .last_timestamp
                .as_ref()
                .map(|t| t.0.to_rfc3339())
                .unwrap_or_default();

            serde_json::json!({
                "namespace": ns,
                "reason": reason,
                "message": message,
                "kind": kind,
                "object": obj_name,
                "type": type_,
                "count": count,
                "last_seen": last,
            })
        })
        .collect();

    let result = serde_json::json!({
        "events": event_info,
        "total": event_info.len(),
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&result).unwrap(),
    )]))
}

/// Get workload summary across all namespaces.
pub async fn get_workloads(
    client: &Client,
    namespace: Option<&str>,
) -> Result<CallToolResult, McpError> {
    let (deployments, statefulsets, daemonsets) = match namespace {
        Some(ns) => {
            let deps: Api<Deployment> = Api::namespaced(client.clone(), ns);
            let sts: Api<StatefulSet> = Api::namespaced(client.clone(), ns);
            let ds: Api<DaemonSet> = Api::namespaced(client.clone(), ns);
            (
                deps.list(&ListParams::default()).await.map_err(tool_error)?,
                sts.list(&ListParams::default()).await.map_err(tool_error)?,
                ds.list(&ListParams::default()).await.map_err(tool_error)?,
            )
        }
        None => {
            let deps: Api<Deployment> = Api::all(client.clone());
            let sts: Api<StatefulSet> = Api::all(client.clone());
            let ds: Api<DaemonSet> = Api::all(client.clone());
            (
                deps.list(&ListParams::default()).await.map_err(tool_error)?,
                sts.list(&ListParams::default()).await.map_err(tool_error)?,
                ds.list(&ListParams::default()).await.map_err(tool_error)?,
            )
        }
    };

    let dep_info: Vec<serde_json::Value> = deployments
        .items
        .iter()
        .map(|d| {
            let name = d.metadata.name.as_deref().unwrap_or("unknown");
            let ns = d.metadata.namespace.as_deref().unwrap_or("unknown");
            let status = d.status.as_ref();
            let ready = status.and_then(|s| s.ready_replicas).unwrap_or(0);
            let desired = d
                .spec
                .as_ref()
                .and_then(|s| s.replicas)
                .unwrap_or(1);
            serde_json::json!({
                "name": name,
                "namespace": ns,
                "ready": ready,
                "desired": desired,
            })
        })
        .collect();

    let sts_info: Vec<serde_json::Value> = statefulsets
        .items
        .iter()
        .map(|s| {
            let name = s.metadata.name.as_deref().unwrap_or("unknown");
            let ns = s.metadata.namespace.as_deref().unwrap_or("unknown");
            let status = s.status.as_ref();
            let ready = status.and_then(|s| s.ready_replicas).unwrap_or(0);
            let desired = s
                .spec
                .as_ref()
                .and_then(|s| s.replicas)
                .unwrap_or(1);
            serde_json::json!({
                "name": name,
                "namespace": ns,
                "ready": ready,
                "desired": desired,
            })
        })
        .collect();

    let ds_info: Vec<serde_json::Value> = daemonsets
        .items
        .iter()
        .map(|d| {
            let name = d.metadata.name.as_deref().unwrap_or("unknown");
            let ns = d.metadata.namespace.as_deref().unwrap_or("unknown");
            let status = d.status.as_ref();
            let ready = status.map(|s| s.number_ready).unwrap_or(0);
            let desired = status
                .map(|s| s.desired_number_scheduled)
                .unwrap_or(0);
            serde_json::json!({
                "name": name,
                "namespace": ns,
                "ready": ready,
                "desired": desired,
            })
        })
        .collect();

    let result = serde_json::json!({
        "deployments": dep_info,
        "statefulsets": sts_info,
        "daemonsets": ds_info,
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&result).unwrap(),
    )]))
}

/// Input schema for tools that accept an optional namespace.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NamespaceFilter {
    /// Kubernetes namespace to filter by. If omitted, returns results from all namespaces.
    pub namespace: Option<String>,
}

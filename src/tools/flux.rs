use kube::api::ListParams;
use kube::Client;
use rmcp::{ErrorData as McpError, model::*};

use super::tool_error;

/// Helper to build an ApiResource for Flux CRDs.
fn flux_resource(group: &str, version: &str, kind: &str, plural: &str) -> kube::discovery::ApiResource {
    kube::discovery::ApiResource {
        group: group.into(),
        version: version.into(),
        api_version: format!("{group}/{version}"),
        kind: kind.into(),
        plural: plural.into(),
    }
}

/// Extract a common status summary from a Flux object's dynamic data.
fn flux_status_summary(data: &serde_json::Value) -> serde_json::Value {
    let ready = data
        .pointer("/status/conditions")
        .and_then(|c| c.as_array())
        .and_then(|conditions| {
            conditions
                .iter()
                .find(|c| c.get("type").and_then(|t| t.as_str()) == Some("Ready"))
        });

    let ready_status = ready
        .and_then(|c| c.get("status"))
        .and_then(|s| s.as_str())
        .unwrap_or("Unknown");

    let ready_message = ready
        .and_then(|c| c.get("message"))
        .and_then(|s| s.as_str())
        .unwrap_or("");

    let last_transition = ready
        .and_then(|c| c.get("lastTransitionTime"))
        .and_then(|s| s.as_str())
        .unwrap_or("unknown");

    serde_json::json!({
        "ready": ready_status,
        "message": ready_message,
        "last_transition": last_transition,
    })
}

/// Get Flux Kustomization and HelmRelease reconciliation status.
pub async fn get_flux_status(client: &Client) -> Result<CallToolResult, McpError> {
    let ks_api = kube::Api::<kube::api::DynamicObject>::all_with(
        client.clone(),
        &flux_resource("kustomize.toolkit.fluxcd.io", "v1", "Kustomization", "kustomizations"),
    );
    let hr_api = kube::Api::<kube::api::DynamicObject>::all_with(
        client.clone(),
        &flux_resource("helm.toolkit.fluxcd.io", "v2", "HelmRelease", "helmreleases"),
    );

    let lp = ListParams::default();
    let (ks_result, hr_result) = tokio::join!(
        ks_api.list(&lp),
        hr_api.list(&lp),
    );

    let kustomizations: Vec<serde_json::Value> = ks_result
        .map_err(tool_error)?
        .items
        .iter()
        .map(|ks| {
            let name = ks.metadata.name.as_deref().unwrap_or("unknown");
            let ns = ks.metadata.namespace.as_deref().unwrap_or("unknown");
            let mut info = flux_status_summary(&ks.data);
            info["name"] = serde_json::json!(name);
            info["namespace"] = serde_json::json!(ns);
            info["source"] = ks.data
                .pointer("/spec/sourceRef/name")
                .cloned()
                .unwrap_or(serde_json::json!("unknown"));
            info["path"] = ks.data
                .pointer("/spec/path")
                .cloned()
                .unwrap_or(serde_json::json!(""));
            info["revision"] = ks.data
                .pointer("/status/lastAppliedRevision")
                .cloned()
                .unwrap_or(serde_json::json!("unknown"));
            info
        })
        .collect();

    let helm_releases: Vec<serde_json::Value> = hr_result
        .map_err(tool_error)?
        .items
        .iter()
        .map(|hr| {
            let name = hr.metadata.name.as_deref().unwrap_or("unknown");
            let ns = hr.metadata.namespace.as_deref().unwrap_or("unknown");
            let mut info = flux_status_summary(&hr.data);
            info["name"] = serde_json::json!(name);
            info["namespace"] = serde_json::json!(ns);
            info["chart"] = hr.data
                .pointer("/spec/chart/spec/chart")
                .cloned()
                .unwrap_or(serde_json::json!("unknown"));
            info["version"] = hr.data
                .pointer("/status/lastAppliedRevision")
                .cloned()
                .unwrap_or(serde_json::json!("unknown"));
            info
        })
        .collect();

    let not_ready: Vec<&serde_json::Value> = kustomizations
        .iter()
        .chain(helm_releases.iter())
        .filter(|obj| obj.get("ready").and_then(|r| r.as_str()) != Some("True"))
        .collect();

    let result = serde_json::json!({
        "kustomizations": kustomizations,
        "helm_releases": helm_releases,
        "summary": {
            "total_kustomizations": kustomizations.len(),
            "total_helm_releases": helm_releases.len(),
            "not_ready_count": not_ready.len(),
        },
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&result).unwrap(),
    )]))
}

/// Get Flux source (GitRepository, HelmRepository, OCIRepository) status.
pub async fn get_flux_sources(client: &Client) -> Result<CallToolResult, McpError> {
    let git_api = kube::Api::<kube::api::DynamicObject>::all_with(
        client.clone(),
        &flux_resource("source.toolkit.fluxcd.io", "v1", "GitRepository", "gitrepositories"),
    );
    let helm_api = kube::Api::<kube::api::DynamicObject>::all_with(
        client.clone(),
        &flux_resource("source.toolkit.fluxcd.io", "v1", "HelmRepository", "helmrepositories"),
    );
    let oci_api = kube::Api::<kube::api::DynamicObject>::all_with(
        client.clone(),
        &flux_resource("source.toolkit.fluxcd.io", "v1", "OCIRepository", "ocirepositories"),
    );

    let lp = ListParams::default();
    let (git_result, helm_result, oci_result) = tokio::join!(
        git_api.list(&lp),
        helm_api.list(&lp),
        oci_api.list(&lp),
    );

    let format_source = |obj: &kube::api::DynamicObject, source_type: &str| -> serde_json::Value {
        let name = obj.metadata.name.as_deref().unwrap_or("unknown");
        let ns = obj.metadata.namespace.as_deref().unwrap_or("unknown");
        let mut info = flux_status_summary(&obj.data);
        info["name"] = serde_json::json!(name);
        info["namespace"] = serde_json::json!(ns);
        info["type"] = serde_json::json!(source_type);
        info["url"] = obj.data
            .pointer("/spec/url")
            .cloned()
            .unwrap_or(serde_json::json!(""));
        info["revision"] = obj.data
            .pointer("/status/artifact/revision")
            .cloned()
            .unwrap_or(serde_json::json!("unknown"));
        info
    };

    let mut sources: Vec<serde_json::Value> = Vec::new();

    for repo in &git_result.map_err(tool_error)?.items {
        sources.push(format_source(repo, "GitRepository"));
    }
    for repo in &helm_result.map_err(tool_error)?.items {
        sources.push(format_source(repo, "HelmRepository"));
    }
    for repo in &oci_result.map_err(tool_error)?.items {
        sources.push(format_source(repo, "OCIRepository"));
    }

    let result = serde_json::json!({
        "sources": sources,
        "total": sources.len(),
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&result).unwrap(),
    )]))
}

/// Get Flux image automation status (ImageRepository, ImagePolicy, ImageUpdateAutomation).
pub async fn get_flux_images(client: &Client) -> Result<CallToolResult, McpError> {
    let repo_api = kube::Api::<kube::api::DynamicObject>::all_with(
        client.clone(),
        &flux_resource("image.toolkit.fluxcd.io", "v1beta2", "ImageRepository", "imagerepositories"),
    );
    let policy_api = kube::Api::<kube::api::DynamicObject>::all_with(
        client.clone(),
        &flux_resource("image.toolkit.fluxcd.io", "v1beta2", "ImagePolicy", "imagepolicies"),
    );
    let auto_api = kube::Api::<kube::api::DynamicObject>::all_with(
        client.clone(),
        &flux_resource("image.toolkit.fluxcd.io", "v1beta2", "ImageUpdateAutomation", "imageupdateautomations"),
    );

    let lp = ListParams::default();
    let (repo_result, policy_result, auto_result) = tokio::join!(
        repo_api.list(&lp),
        policy_api.list(&lp),
        auto_api.list(&lp),
    );

    let image_repos: Vec<serde_json::Value> = repo_result
        .map_err(tool_error)?
        .items
        .iter()
        .map(|obj| {
            let name = obj.metadata.name.as_deref().unwrap_or("unknown");
            let ns = obj.metadata.namespace.as_deref().unwrap_or("unknown");
            let mut info = flux_status_summary(&obj.data);
            info["name"] = serde_json::json!(name);
            info["namespace"] = serde_json::json!(ns);
            info["image"] = obj.data
                .pointer("/spec/image")
                .cloned()
                .unwrap_or(serde_json::json!("unknown"));
            info["last_scan"] = obj.data
                .pointer("/status/lastScanResult/scanTime")
                .cloned()
                .unwrap_or(serde_json::json!("unknown"));
            info["tag_count"] = obj.data
                .pointer("/status/lastScanResult/tagCount")
                .cloned()
                .unwrap_or(serde_json::json!(0));
            info
        })
        .collect();

    let image_policies: Vec<serde_json::Value> = policy_result
        .map_err(tool_error)?
        .items
        .iter()
        .map(|obj| {
            let name = obj.metadata.name.as_deref().unwrap_or("unknown");
            let ns = obj.metadata.namespace.as_deref().unwrap_or("unknown");
            let mut info = flux_status_summary(&obj.data);
            info["name"] = serde_json::json!(name);
            info["namespace"] = serde_json::json!(ns);
            info["latest_image"] = obj.data
                .pointer("/status/latestImage")
                .cloned()
                .unwrap_or(serde_json::json!("unknown"));
            info
        })
        .collect();

    let automations: Vec<serde_json::Value> = auto_result
        .map_err(tool_error)?
        .items
        .iter()
        .map(|obj| {
            let name = obj.metadata.name.as_deref().unwrap_or("unknown");
            let ns = obj.metadata.namespace.as_deref().unwrap_or("unknown");
            let mut info = flux_status_summary(&obj.data);
            info["name"] = serde_json::json!(name);
            info["namespace"] = serde_json::json!(ns);
            info["last_push_commit"] = obj.data
                .pointer("/status/lastPushCommit")
                .cloned()
                .unwrap_or(serde_json::json!("unknown"));
            info["last_push_time"] = obj.data
                .pointer("/status/lastPushTime")
                .cloned()
                .unwrap_or(serde_json::json!("unknown"));
            info
        })
        .collect();

    let result = serde_json::json!({
        "image_repositories": image_repos,
        "image_policies": image_policies,
        "image_update_automations": automations,
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&result).unwrap(),
    )]))
}

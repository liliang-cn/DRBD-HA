use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::core::run_shell_command;
use crate::error::{AppError, AppResult};
use crate::models::{Node, NodeStatus};
use crate::state::{AppState, NotificationLevel};
use drbd_reactor_utils::{DrbdReactorClient, EvictOptions};

use super::types::{EvictProfileRequest, EvictProfileResponse};
use super::utils::create_profile_from_toml;

/// Resolve hostname to IP address using DNS lookup
fn resolve_hostname_to_ip(hostname: &str) -> Option<String> {
    drbd_utils::resolve_hostname_to_ip(hostname)
}

/// Auto-discover and add a node to the store if it doesn't exist
fn ensure_node_in_store(state: &AppState, hostname: &str) -> AppResult<Node> {
    // First, try to find existing node
    let nodes = state.node_store.get_all()?;
    if let Some(node) = nodes.into_iter().find(|n| n.hostname == hostname) {
        return Ok(node);
    }

    // Node not found, auto-discover and add it
    tracing::info!("Auto-discovering node '{}' and adding to store", hostname);

    let resolved_ip = resolve_hostname_to_ip(hostname);
    let is_local = state.matches_controller_target(
        hostname,
        resolved_ip.as_deref().unwrap_or(hostname),
        hostname,
    );

    let ip = if is_local {
        resolved_ip.unwrap_or_else(crate::state::AppState::get_local_ip)
    } else {
        // Try DNS resolution
        resolved_ip.ok_or_else(|| {
            AppError::NotFound(format!("Cannot resolve IP for hostname '{}'", hostname))
        })?
    };

    let ssh_port = state.config.ssh.default_port;
    let ssh_user = state.config.ssh.default_user.clone();

    let new_node = Node {
        id: hostname.to_string(),
        hostname: hostname.to_string(),
        ip,
        ssh_port,
        ssh_user,
        is_local,
        status: NodeStatus::Online,
        status_message: None,
        last_seen: Some(chrono::Utc::now()),
    };

    // Add to store
    state.node_store.insert(&new_node)?;

    tracing::info!(
        "Auto-added node '{}' (IP: {}, SSH: {}@{}:{}) to store",
        hostname,
        new_node.ip,
        new_node.ssh_user,
        new_node.ip,
        new_node.ssh_port
    );

    Ok(new_node)
}

/// POST /api/v1/ha/profiles/:id/evict
#[utoipa::path(
    post,
    path = "/api/v1/ha/profiles/{id}/evict",
    tag = "ha",
    params(
        ("id" = String, Path, description = "Profile ID or name")
    ),
    request_body = EvictProfileRequest,
    responses(
        (status = 200, description = "Evict operation result", body = EvictProfileResponse),
        (status = 404, description = "Profile not found")
    )
)]
pub async fn evict_profile(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
    Json(request): Json<EvictProfileRequest>,
) -> AppResult<Json<EvictProfileResponse>> {
    // Load profile from toml file instead of database
    let config_path = crate::core::ReactorConfigPaths::promoter_path(&id_or_name);
    let content = state
        .read_controller_file(&config_path)
        .await
        .map_err(|_| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;
    let profile = create_profile_from_toml(&id_or_name, &content)
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    // Build evict options from request
    let mut evict_options = EvictOptions::default();

    // Only set delay if not default (20)
    if request.delay != 20 {
        evict_options.delay = Some(request.delay);
    }
    evict_options.force = request.force;
    evict_options.keep_masked = request.keep_masked;

    // Build command using drbd-reactor-utils
    let evict_cmd = DrbdReactorClient::build_evict_command(&profile.name, Some(&evict_options));

    let target_node: Node = if let Some(ref node_id) = request.node {
        // Try to find existing node first
        let nodes = state.node_store.get_all()?;
        if let Some(node) = nodes
            .into_iter()
            .find(|n| n.id == *node_id || n.hostname == *node_id)
        {
            node
        } else {
            // Auto-discover and add node if not in store
            ensure_node_in_store(&state, node_id)?
        }
    } else {
        let active_node_name = DrbdReactorClient::status(Some(&profile.name), None)
            .await
            .ok()
            .and_then(|(statuses, _)| statuses.into_iter().next())
            .and_then(|status| status.active_node);

        if let Some(active_hostname) = active_node_name {
            // Auto-discover and add node if not in store
            ensure_node_in_store(&state, &active_hostname)?
        } else {
            return Err(AppError::Validation(
                "No active node found for this profile. Cannot evict.".to_string(),
            ));
        }
    };

    let operation_id = uuid::Uuid::new_v4().to_string();
    state.send_progress(
        &operation_id,
        "evict_profile",
        Some(&profile.name),
        0,
        &format!("Evicting from node {}...", target_node.hostname),
        false,
        None,
    );

    tracing::info!(
        "Evicting HA profile {} from node {} (delay: {}s, keep_masked: {}, force: {})",
        profile.name,
        target_node.hostname,
        request.delay,
        request.keep_masked,
        request.force
    );

    // Execute the evict directly on the target (active) node via dispatch.
    // It must run *on the node being evicted* — never through the global
    // shell-cmd proxy, which would run it on the wrong node and silently
    // no-op ("nothing to do on this node").
    tracing::info!(
        "Executing evict on node {} ({}): {}",
        target_node.hostname,
        target_node.ip,
        evict_cmd
    );
    let (success, stdout, stderr) =
        match crate::core::dispatch_client::DispatchClient::exec(&target_node, &evict_cmd).await {
            Ok(hr) => {
                tracing::info!(
                    "Evict on {} completed with exit code {}",
                    target_node.hostname,
                    hr.exit_code
                );
                (hr.success, Some(hr.stdout), Some(hr.stderr))
            }
            Err(e) => {
                tracing::error!("Failed to execute evict on {}: {}", target_node.hostname, e);
                return Err(AppError::Ssh(format!(
                    "Failed to execute evict on {}: {}",
                    target_node.hostname, e
                )));
            }
        };

    let message = if success {
        state.send_progress(
            &operation_id,
            "evict_profile",
            Some(&profile.name),
            100,
            &format!(
                "Evicted from {}. Failover in progress...",
                target_node.hostname
            ),
            true,
            Some(true),
        );
        state.send_notification(
            NotificationLevel::Warning,
            "Profile Evicted",
            &format!(
                "HA profile '{}' evicted from {}. Failover initiated.",
                profile.name, target_node.hostname
            ),
        );
        format!(
            "Successfully evicted {} from node {}. Another node should take over within {} seconds.",
            profile.name,
            target_node.hostname,
            request.delay
        )
    } else {
        state.send_progress(
            &operation_id,
            "evict_profile",
            Some(&profile.name),
            100,
            &format!(
                "Evict failed: {}",
                stderr.as_deref().unwrap_or("unknown error")
            ),
            true,
            Some(false),
        );
        state.send_notification(
            NotificationLevel::Error,
            "Evict Failed",
            &format!(
                "Failed to evict '{}' from {}",
                profile.name, target_node.hostname
            ),
        );
        format!(
            "Failed to evict {} from node {}: {}",
            profile.name,
            target_node.hostname,
            stderr.as_deref().unwrap_or("unknown error")
        )
    };

    tracing::info!("{}", message);

    Ok(Json(EvictProfileResponse {
        success,
        node: target_node.hostname,
        profile: profile.name,
        message,
        stdout,
        stderr,
    }))
}

/// POST /api/v1/ha/profiles/:id/:node/disable
#[utoipa::path(
    post,
    path = "/api/v1/ha/profiles/{id}/{node}/disable",
    tag = "ha",
    params(
        ("id" = String, Path, description = "Profile ID or name"),
        ("node" = String, Path, description = "Node hostname")
    ),
    responses(
        (status = 200, description = "Profile disabled on node", body = serde_json::Value),
        (status = 404, description = "Profile or node not found")
    )
)]
pub async fn disable_profile_on_node(
    State(state): State<Arc<AppState>>,
    Path((id_or_name, node_hostname)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    // Get node info (auto-discover if not in store)
    let target_node = ensure_node_in_store(&state, &node_hostname)?;

    // Verify profile exists (check both .toml and .toml.disabled)
    let reactor_dir = crate::core::ReactorConfigPaths::REACTOR_CONF_DIR;
    let config_path = std::path::Path::new(reactor_dir).join(format!("{}.toml", id_or_name));
    let disabled_path =
        std::path::Path::new(reactor_dir).join(format!("{}.toml.disabled", id_or_name));

    if !config_path.exists() && !disabled_path.exists() {
        return Err(AppError::NotFound(format!(
            "HA profile {} not found",
            id_or_name
        )));
    }

    tracing::info!(
        "Disabling HA profile {} on node {}",
        id_or_name,
        target_node.hostname
    );

    let disable_cmd = format!("sudo drbd-reactorctl disable {}", id_or_name);

    let result = if state.is_controller_node(&target_node) {
        run_shell_command(
            &disable_cmd,
            &format!("Disable profile {} locally", id_or_name),
        )
        .await
    } else {
        let credential = crate::core::SshCredential::Password("ignored".to_string());
        state
            .ssh_manager
            .execute(
                &target_node.ip,
                target_node.ssh_port,
                &target_node.ssh_user,
                &credential,
                &disable_cmd,
            )
            .await
            .map_err(|e| AppError::Ssh(format!("SSH execution failed: {}", e)))
    };

    match result {
        Ok(output) => {
            if output.success() {
                Ok(Json(serde_json::json!({
                    "success": true,
                    "message": format!("Profile '{}' disabled on node '{}'", id_or_name, target_node.hostname),
                    "node": target_node.hostname,
                    "profile": id_or_name
                })))
            } else {
                Err(AppError::Internal(format!(
                    "Failed to disable profile: {}",
                    output.stderr
                )))
            }
        }
        Err(e) => Err(e),
    }
}

/// POST /api/v1/ha/profiles/:id/enable
#[utoipa::path(
    post,
    path = "/api/v1/ha/profiles/{id}/enable",
    tag = "ha",
    params(
        ("id" = String, Path, description = "Profile ID or name")
    ),
    responses(
        (status = 200, description = "Profile enabled on all nodes", body = serde_json::Value),
        (status = 404, description = "Profile not found")
    )
)]
pub async fn enable_profile(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    // Verify profile exists (check both .toml and .toml.disabled)
    let reactor_dir = crate::core::ReactorConfigPaths::REACTOR_CONF_DIR;
    let config_path = std::path::Path::new(reactor_dir).join(format!("{}.toml", id_or_name));
    let disabled_path =
        std::path::Path::new(reactor_dir).join(format!("{}.toml.disabled", id_or_name));

    if !config_path.exists() && !disabled_path.exists() {
        return Err(AppError::NotFound(format!(
            "HA profile {} not found",
            id_or_name
        )));
    }

    // Get all nodes
    let nodes = state.node_store.get_all()?;

    tracing::info!("Enabling HA profile {} on all nodes", id_or_name);

    let enable_cmd = format!("sudo drbd-reactorctl enable {}", id_or_name);
    let mut enabled_nodes = Vec::new();
    let mut failed_nodes = Vec::new();

    for node in &nodes {
        let result = if state.is_controller_node(node) {
            run_shell_command(
                &enable_cmd,
                &format!("Enable profile {} locally", id_or_name),
            )
            .await
        } else {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &enable_cmd,
                )
                .await
                .map_err(|e| AppError::Ssh(format!("SSH execution failed: {}", e)))
        };

        match result {
            Ok(output) if output.success() => {
                tracing::info!(
                    "Successfully enabled profile {} on node {}",
                    id_or_name,
                    node.hostname
                );
                enabled_nodes.push(node.hostname.clone());
            }
            Ok(output) => {
                tracing::warn!(
                    "Failed to enable profile {} on node {}: {}",
                    id_or_name,
                    node.hostname,
                    output.stderr
                );
                failed_nodes.push((node.hostname.clone(), output.stderr));
            }
            Err(e) => {
                tracing::error!(
                    "Error enabling profile {} on node {}: {}",
                    id_or_name,
                    node.hostname,
                    e
                );
                failed_nodes.push((node.hostname.clone(), e.to_string()));
            }
        }
    }

    let success = !enabled_nodes.is_empty();
    let message = if failed_nodes.is_empty() {
        format!(
            "Profile '{}' enabled on all {} nodes",
            id_or_name,
            enabled_nodes.len()
        )
    } else if enabled_nodes.is_empty() {
        format!("Failed to enable profile '{}' on any node", id_or_name)
    } else {
        format!(
            "Profile '{}' enabled on {}/{} nodes",
            id_or_name,
            enabled_nodes.len(),
            nodes.len()
        )
    };

    Ok(Json(serde_json::json!({
        "success": success,
        "message": message,
        "profile": id_or_name,
        "enabled_nodes": enabled_nodes,
        "failed_nodes": failed_nodes
    })))
}

/// POST /api/v1/ha/profiles/:id/:node/enable
#[utoipa::path(
    post,
    path = "/api/v1/ha/profiles/{id}/{node}/enable",
    tag = "ha",
    params(
        ("id" = String, Path, description = "Profile ID or name"),
        ("node" = String, Path, description = "Node hostname")
    ),
    responses(
        (status = 200, description = "Profile enabled on node", body = serde_json::Value),
        (status = 404, description = "Profile or node not found")
    )
)]
pub async fn enable_profile_on_node(
    State(state): State<Arc<AppState>>,
    Path((id_or_name, node_hostname)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    // Get node info (auto-discover if not in store)
    let target_node = ensure_node_in_store(&state, &node_hostname)?;

    // Verify profile exists (check both .toml and .toml.disabled)
    let reactor_dir = crate::core::ReactorConfigPaths::REACTOR_CONF_DIR;
    let config_path = std::path::Path::new(reactor_dir).join(format!("{}.toml", id_or_name));
    let disabled_path =
        std::path::Path::new(reactor_dir).join(format!("{}.toml.disabled", id_or_name));

    if !config_path.exists() && !disabled_path.exists() {
        return Err(AppError::NotFound(format!(
            "HA profile {} not found",
            id_or_name
        )));
    }

    tracing::info!(
        "Enabling HA profile {} on node {}",
        id_or_name,
        target_node.hostname
    );

    let enable_cmd = format!("sudo drbd-reactorctl enable {}", id_or_name);

    let result = if state.is_controller_node(&target_node) {
        run_shell_command(
            &enable_cmd,
            &format!("Enable profile {} locally", id_or_name),
        )
        .await
    } else {
        let credential = crate::core::SshCredential::Password("ignored".to_string());
        state
            .ssh_manager
            .execute(
                &target_node.ip,
                target_node.ssh_port,
                &target_node.ssh_user,
                &credential,
                &enable_cmd,
            )
            .await
            .map_err(|e| AppError::Ssh(format!("SSH execution failed: {}", e)))
    };

    match result {
        Ok(output) => {
            if output.success() {
                Ok(Json(serde_json::json!({
                    "success": true,
                    "message": format!("Profile '{}' enabled on node '{}'", id_or_name, target_node.hostname),
                    "node": target_node.hostname,
                    "profile": id_or_name
                })))
            } else {
                Err(AppError::Internal(format!(
                    "Failed to enable profile: {}",
                    output.stderr
                )))
            }
        }
        Err(e) => Err(e),
    }
}

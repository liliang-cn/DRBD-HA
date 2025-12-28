use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::core::run_shell_command;
use crate::error::{AppError, AppResult};
use crate::state::{AppState, NotificationLevel};
use drbd_reactor_utils::{DrbdReactorClient, EvictOptions};

use super::types::{EvictProfileRequest, EvictProfileResponse};
use super::utils::create_profile_from_toml;

/// POST /api/v1/ha/profiles/:id/evict
pub async fn evict_profile(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
    Json(request): Json<EvictProfileRequest>,
) -> AppResult<Json<EvictProfileResponse>> {
    // Load profile from toml file instead of database
    let config_path = crate::core::ReactorConfigPaths::promoter_path(&id_or_name);
    let content = tokio::fs::read_to_string(&config_path)
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

    let target_node: crate::models::Node = if let Some(ref node_id) = request.node {
        let nodes = state.node_store.get_all()?;
        nodes
            .into_iter()
            .find(|n| n.id == *node_id || n.hostname == *node_id)
            .ok_or_else(|| AppError::NotFound(format!("Node {} not found", node_id)))?
    } else {
        let status_cmd = format!("drbd-reactorctl status {} 2>/dev/null", profile.name);
        let output = run_shell_command(
            &status_cmd,
            &format!("Get drbd-reactorctl status for profile {}", profile.name),
        )
        .await?;

        let active_node_name = if output.success() && !output.stdout.is_empty() {
            output
                .stdout
                .lines()
                .find(|line| line.contains("Currently active"))
                .and_then(|line| {
                    if line.contains("Currently active on this node") {
                        Some(gethostname::gethostname().to_string_lossy().to_string())
                    } else if line.contains("Currently active on node") {
                        let parts: Vec<&str> = line.split("Currently active on node").collect();
                        if let Some(suffix) = parts.get(1) {
                            let node = suffix
                                .trim_start_matches([':', ' ', '\''])
                                .trim_end_matches('\'')
                                .trim();
                            if !node.is_empty() {
                                Some(node.to_string())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
        } else {
            None
        };

        if let Some(active_hostname) = active_node_name {
            let nodes = state.node_store.get_all()?;
            nodes
                .into_iter()
                .find(|n| n.hostname == active_hostname)
                .ok_or_else(|| {
                    AppError::NotFound(format!(
                        "Active node '{}' not found in store. Please add it first.",
                        active_hostname
                    ))
                })?
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

    let (success, stdout, stderr) = if target_node.is_local {
        let output = run_shell_command(
            &evict_cmd,
            &format!("Evict HA profile {} from local node", profile.name),
        )
        .await?;
        (output.success(), Some(output.stdout), Some(output.stderr))
    } else {
        let credential = Some(crate::core::SshCredential::Password("ignored".to_string()));

        if let Some(cred) = credential {
            match state
                .ssh_manager
                .execute(
                    &target_node.ip,
                    target_node.ssh_port,
                    &target_node.ssh_user,
                    &cred,
                    &evict_cmd,
                )
                .await
            {
                Ok(output) => (output.success(), Some(output.stdout), Some(output.stderr)),
                Err(e) => {
                    return Err(AppError::Ssh(format!(
                        "Failed to execute evict on {}: {}",
                        target_node.hostname, e
                    )));
                }
            }
        } else {
            return Err(AppError::Ssh(format!(
                "No credentials available for node {}",
                target_node.hostname
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
pub async fn disable_profile_on_node(
    State(state): State<Arc<AppState>>,
    Path((id_or_name, node_hostname)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    // Get node info
    let nodes = state.node_store.get_all()?;
    let target_node = nodes
        .into_iter()
        .find(|n| n.hostname == node_hostname || n.id == node_hostname)
        .ok_or_else(|| AppError::NotFound(format!("Node {} not found", node_hostname)))?;

    // Verify profile exists (check both .toml and .toml.disabled)
    let reactor_dir = crate::core::ReactorConfigPaths::REACTOR_CONF_DIR;
    let config_path = std::path::Path::new(reactor_dir).join(format!("{}.toml", id_or_name));
    let disabled_path = std::path::Path::new(reactor_dir).join(format!("{}.toml.disabled", id_or_name));

    if !config_path.exists() && !disabled_path.exists() {
        return Err(AppError::NotFound(format!("HA profile {} not found", id_or_name)));
    }

    tracing::info!("Disabling HA profile {} on node {}", id_or_name, target_node.hostname);

    let disable_cmd = format!("sudo drbd-reactorctl disable {}", id_or_name);

    let result = if target_node.is_local {
        run_shell_command(&disable_cmd, &format!("Disable profile {} locally", id_or_name)).await
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
pub async fn enable_profile(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    // Verify profile exists (check both .toml and .toml.disabled)
    let reactor_dir = crate::core::ReactorConfigPaths::REACTOR_CONF_DIR;
    let config_path = std::path::Path::new(reactor_dir).join(format!("{}.toml", id_or_name));
    let disabled_path = std::path::Path::new(reactor_dir).join(format!("{}.toml.disabled", id_or_name));

    if !config_path.exists() && !disabled_path.exists() {
        return Err(AppError::NotFound(format!("HA profile {} not found", id_or_name)));
    }

    // Get all nodes
    let nodes = state.node_store.get_all()?;

    tracing::info!("Enabling HA profile {} on all nodes", id_or_name);

    let enable_cmd = format!("sudo drbd-reactorctl enable {}", id_or_name);
    let mut enabled_nodes = Vec::new();
    let mut failed_nodes = Vec::new();

    for node in &nodes {
        let result = if node.is_local {
            run_shell_command(&enable_cmd, &format!("Enable profile {} locally", id_or_name)).await
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
                tracing::info!("Successfully enabled profile {} on node {}", id_or_name, node.hostname);
                enabled_nodes.push(node.hostname.clone());
            }
            Ok(output) => {
                tracing::warn!("Failed to enable profile {} on node {}: {}", id_or_name, node.hostname, output.stderr);
                failed_nodes.push((node.hostname.clone(), output.stderr));
            }
            Err(e) => {
                tracing::error!("Error enabling profile {} on node {}: {}", id_or_name, node.hostname, e);
                failed_nodes.push((node.hostname.clone(), e.to_string()));
            }
        }
    }

    let success = enabled_nodes.len() > 0;
    let message = if failed_nodes.is_empty() {
        format!("Profile '{}' enabled on all {} nodes", id_or_name, enabled_nodes.len())
    } else if enabled_nodes.is_empty() {
        format!("Failed to enable profile '{}' on any node", id_or_name)
    } else {
        format!("Profile '{}' enabled on {}/{} nodes", id_or_name, enabled_nodes.len(), nodes.len())
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
pub async fn enable_profile_on_node(
    State(state): State<Arc<AppState>>,
    Path((id_or_name, node_hostname)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    // Get node info
    let nodes = state.node_store.get_all()?;
    let target_node = nodes
        .into_iter()
        .find(|n| n.hostname == node_hostname || n.id == node_hostname)
        .ok_or_else(|| AppError::NotFound(format!("Node {} not found", node_hostname)))?;

    // Verify profile exists (check both .toml and .toml.disabled)
    let reactor_dir = crate::core::ReactorConfigPaths::REACTOR_CONF_DIR;
    let config_path = std::path::Path::new(reactor_dir).join(format!("{}.toml", id_or_name));
    let disabled_path = std::path::Path::new(reactor_dir).join(format!("{}.toml.disabled", id_or_name));

    if !config_path.exists() && !disabled_path.exists() {
        return Err(AppError::NotFound(format!("HA profile {} not found", id_or_name)));
    }

    tracing::info!("Enabling HA profile {} on node {}", id_or_name, target_node.hostname);

    let enable_cmd = format!("sudo drbd-reactorctl enable {}", id_or_name);

    let result = if target_node.is_local {
        run_shell_command(&enable_cmd, &format!("Enable profile {} locally", id_or_name)).await
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

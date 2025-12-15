use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::core::run_shell_command;
use crate::error::{AppError, AppResult};
use crate::state::{AppState, NotificationLevel};

use super::types::{EvictProfileRequest, EvictProfileResponse};

/// POST /api/v1/ha/profiles/:id/evict
pub async fn evict_profile(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
    Json(request): Json<EvictProfileRequest>,
) -> AppResult<Json<EvictProfileResponse>> {
    let profile = state
        .db
        .get_ha_profile(&id_or_name)?
        .or(state.db.get_ha_profile_by_name(&id_or_name)?)
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    let mut evict_cmd = format!("sudo drbd-reactorctl evict {}", profile.name);

    if request.delay != 20 {
        evict_cmd.push_str(&format!(" --delay {}", request.delay));
    }
    if request.keep_masked {
        evict_cmd.push_str(" --keep-masked");
    }
    if request.force {
        evict_cmd.push_str(" --force");
    }

    let target_node: crate::models::Node = if let Some(ref node_id) = request.node {
        let nodes = state.db.get_all_nodes()?;
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
            let nodes = state.db.get_all_nodes()?;
            nodes
                .into_iter()
                .find(|n| n.hostname == active_hostname)
                .ok_or_else(|| {
                    AppError::NotFound(format!(
                        "Active node '{}' not found in database. Please add it first.",
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

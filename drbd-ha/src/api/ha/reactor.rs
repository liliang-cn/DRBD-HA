use axum::{
    extract::{Query, State},
    Json,
};
use std::sync::Arc;

use crate::core::{
    run_shell_command,
    systemd_ctrl::{RemoteSystemdController, SystemdController},
    ReactorDiscovery,
};
use crate::error::{AppError, AppResult};
use crate::models::HaProfile;
use crate::state::AppState;

use super::types::{
    ImportProfilesRequest, ImportProfilesResponse, NodeReloadResult, ReactorLogsQuery,
    ReactorLogsResponse, ReactorReloadRequest, ReactorReloadResponse,
};

/// GET /api/v1/ha/reactor/status
pub async fn reactor_status(
    State(_state): State<Arc<AppState>>,
) -> AppResult<Json<serde_json::Value>> {
    let systemd = SystemdController::new().await?;
    let status = systemd.status("drbd-reactor.service").await?;
    Ok(Json(serde_json::json!({
        "service": "drbd-reactor.service",
        "active_state": status.active_state,
        "sub_state": status.sub_state,
        "running": status.is_running(),
        "description": status.description
    })))
}

/// GET /api/v1/ha/reactor/logs
pub async fn reactor_logs(
    Query(query): Query<ReactorLogsQuery>,
) -> AppResult<Json<ReactorLogsResponse>> {
    let lines = query.lines.min(1000);

    let mut cmd = format!("journalctl -u drbd-reactor.service -n {} --no-pager", lines);

    if let Some(since) = &query.since {
        if since
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == ':' || c == ' ')
        {
            cmd.push_str(&format!(" --since '{}'", since));
        }
    }

    let output = run_shell_command(&cmd, "Get drbd-reactor logs").await?;

    let log_lines: Vec<String> = output.stdout.lines().map(|s| s.to_string()).collect();

    Ok(Json(ReactorLogsResponse {
        service: "drbd-reactor.service".to_string(),
        total_lines: log_lines.len(),
        lines: log_lines,
    }))
}

/// POST /api/v1/ha/reactor/reload
pub async fn reload_reactor(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ReactorReloadRequest>,
) -> AppResult<Json<ReactorReloadResponse>> {
    let action = match request.action.as_str() {
        "restart" => "restart",
        _ => "reload",
    };

    let mut results = Vec::new();

    let local_hostname = gethostname::gethostname().to_string_lossy().to_string();

    let (local_success, local_error) = match SystemdController::new().await {
        Ok(sys) => {
            if let Err(e) = sys.daemon_reload().await {
                (false, Some(e.to_string()))
            } else {
                let res = if action == "restart" {
                    sys.restart("drbd-reactor.service").await
                } else {
                    sys.reload("drbd-reactor.service").await
                };
                match res {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
        }
        Err(e) => (false, Some(e.to_string())),
    };

    let local_node_result = NodeReloadResult {
        hostname: local_hostname.clone(),
        success: local_success,
        error: local_error,
    };

    let nodes = state.node_store.get_all()?;

    for node in nodes {
        if node.is_local {
            continue;
        }

        let credential = Some(crate::core::SshCredential::Password("ignored".to_string()));

        let result = if let Some(cred) = credential {
            let remote_sys = RemoteSystemdController::new(state.ssh_manager.clone());
            match remote_sys
                .daemon_reload(&node.ip, node.ssh_port, &node.ssh_user, &cred)
                .await
            {
                Ok(_) => {
                    let res = if action == "restart" {
                        remote_sys
                            .restart(
                                &node.ip,
                                node.ssh_port,
                                &node.ssh_user,
                                &cred,
                                "drbd-reactor.service",
                            )
                            .await
                    } else {
                        remote_sys
                            .reload(
                                &node.ip,
                                node.ssh_port,
                                &node.ssh_user,
                                &cred,
                                "drbd-reactor.service",
                            )
                            .await
                    };
                    match res {
                        Ok(_) => NodeReloadResult {
                            hostname: node.hostname.clone(),
                            success: true,
                            error: None,
                        },
                        Err(e) => NodeReloadResult {
                            hostname: node.hostname.clone(),
                            success: false,
                            error: Some(e.to_string()),
                        },
                    }
                }
                Err(e) => NodeReloadResult {
                    hostname: node.hostname.clone(),
                    success: false,
                    error: Some(e.to_string()),
                },
            }
        } else {
            NodeReloadResult {
                hostname: node.hostname.clone(),
                success: false,
                error: Some("No credentials available".to_string()),
            }
        };

        results.push(result);
    }

    let success_count =
        results.iter().filter(|r| r.success).count() + if local_success { 1 } else { 0 };
    let total_count = results.len() + 1;

    let message = if success_count == total_count {
        format!(
            "Successfully {}ed drbd-reactor on all {} nodes",
            action, total_count
        )
    } else {
        format!(
            "{}ed drbd-reactor on {}/{} nodes",
            action, success_count, total_count
        )
    };

    tracing::info!("{}", message);

    Ok(Json(ReactorReloadResponse {
        local: local_node_result,
        remote_nodes: results,
        message,
    }))
}

/// GET /api/v1/ha/unmanaged
#[utoipa::path(
    get,
    path = "/api/v1/ha/unmanaged",
    responses(
        (status = 200, description = "List of unmanaged profiles", body = [HaProfile])
    )
)]
pub async fn list_unmanaged_profiles(
    State(_state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<HaProfile>>> {
    tracing::info!("Scanning for unmanaged HA profiles");
    let discovered = match ReactorDiscovery::scan_profiles() {
        Ok(profiles) => {
            tracing::info!("Found {} profiles in /etc/drbd-reactor.d", profiles.len());
            profiles
        }
        Err(e) => {
            tracing::error!("Failed to scan ReactorDiscovery: {}", e);
            return Err(AppError::Internal(e.to_string()));
        }
    };

    // Get existing profiles from toml files instead of database
    let existing_profiles = super::utils::get_all_ha_profile_names().await?;
    tracing::info!(
        "Found {} existing profiles in toml files",
        existing_profiles.len()
    );

    let unmanaged: Vec<HaProfile> = discovered
        .into_iter()
        .filter(|d| {
            let is_unmanaged = !existing_profiles.contains(&d.name);
            if is_unmanaged {
                tracing::info!("Profile '{}' is unmanaged", d.name);
            }
            is_unmanaged
        })
        .collect();

    tracing::info!("Returning {} unmanaged profiles", unmanaged.len());
    Ok(Json(unmanaged))
}

/// POST /api/v1/ha/import
#[utoipa::path(
    post,
    path = "/api/v1/ha/import",
    tag = "ha",
    request_body = ImportProfilesRequest,
    responses(
        (status = 200, description = "Import result", body = ImportProfilesResponse)
    )
)]
pub async fn import_profiles(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<ImportProfilesRequest>,
) -> AppResult<Json<ImportProfilesResponse>> {
    let discovered = ReactorDiscovery::scan_profiles().map_err(|e| AppError::Internal(e.to_string()))?;
    let existing_profiles = super::utils::get_all_ha_profile_names().await?;

    let mut imported = Vec::new();
    let mut failed = Vec::new();

    for name in req.names {
        if existing_profiles.contains(&name) {
            failed.push(format!("{}: Already managed", name));
            continue;
        }

        if let Some(_profile) = discovered.iter().find(|d| d.name == name) {
            // Since profiles are already in toml files (discovered), just mark as imported
            // No database insertion needed
            imported.push(name);
        } else {
            failed.push(format!("{}: Not found in discovered profiles", name));
        }
    }

    Ok(Json(ImportProfilesResponse { imported, failed }))
}

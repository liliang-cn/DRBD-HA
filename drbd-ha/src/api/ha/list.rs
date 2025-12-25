//! HA Profile list and query operations
//!
//! Handles listing profiles, fetching profile details, and status queries.

use axum::{extract::{Path, State}, Json};
use drbd_reactor_utils::DrbdReactorClient;
use std::sync::Arc;

use crate::core::{
    run_shell_command,
    systemd_ctrl::SystemdController,
    ReactorConfigPaths as ConfigPaths,
};
use crate::error::AppResult;
use crate::state::AppState;

use super::types::{ConfigVisibility, HaProfileDetailResponse, HaProfileListResponse, ServiceStatusInfo};
use super::utils::{create_profile_from_toml, get_all_ha_profile_names};

use gethostname;

/// Get the actual device name for a DRBD resource from its configuration file
/// This is the authoritative source for device names
#[allow(dead_code)]
async fn get_drbd_device_for_resource(resource_name: &str) -> Option<String> {
    let config_path = format!("/etc/drbd.d/{}.res", resource_name);

    match tokio::fs::read_to_string(&config_path).await {
        Ok(content) => {
            for line in content.lines() {
                let trimmed_line = line.trim();
                if trimmed_line.starts_with("device ") {
                    let device_part = trimmed_line
                        .strip_prefix("device ")
                        .unwrap_or("")
                        .trim_end_matches(';');
                    return Some(device_part.to_string());
                }
            }
            tracing::warn!("No device line found in DRBD config for {}", resource_name);
            None
        }
        Err(e) => {
            tracing::debug!("Cannot read DRBD config for {}: {}", resource_name, e);
            None
        }
    }
}

/// GET /api/v1/ha/profiles
pub async fn list_profiles(
    State(_state): State<Arc<AppState>>,
) -> AppResult<Json<HaProfileListResponse>> {
    // Read all profiles from toml files
    let profile_names = get_all_ha_profile_names().await?;
    let mut profiles = Vec::new();

    for name in &profile_names {
        let config_path = ConfigPaths::promoter_path(name);
        if let Ok(content) = tokio::fs::read_to_string(&config_path).await {
            if let Some(mut profile) = create_profile_from_toml(name, &content) {
                // Update status from drbd-reactorctl
                if let Ok((statuses, _)) = DrbdReactorClient::status(Some(name)).await {
                    if let Some(status) = statuses.first() {
                        if status.is_active {
                            profile.status = crate::models::HaProfileStatus::Active;
                            profile.active_node = status.active_node.clone();
                        } else {
                            profile.status = crate::models::HaProfileStatus::Standby;
                            profile.active_node = None;
                        }
                    }
                }
                profiles.push(profile);
            }
        }
    }

    Ok(Json(HaProfileListResponse { profiles }))
}

/// GET /api/v1/ha/profiles/:id
pub async fn get_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<HaProfileDetailResponse>> {
    fetch_profile_details(state, id).await
}

/// Helper function to fetch detailed profile status
async fn fetch_profile_details(
    state: Arc<AppState>,
    id_or_name: String,
) -> AppResult<Json<HaProfileDetailResponse>> {
    use crate::models::HaProfileStatus;
    use crate::core::SshCredential;

    // Load profile from toml file
    let config_path = ConfigPaths::promoter_path(&id_or_name);
    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|_| crate::error::AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    let profile = create_profile_from_toml(&id_or_name, &content)
        .ok_or_else(|| crate::error::AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    // Step 1: Get DRBD status to determine which node is Primary (active)
    // Use drbdsetup status --json for complete status including connection state
    let local_drbd_status = {
        let cmd = format!("drbdsetup status {} --json 2>/dev/null", profile.resource_name);
        let output = run_shell_command(
            &cmd,
            &format!("Get local DRBD status for {}", profile.resource_name),
        )
        .await?;

        tracing::debug!("drbdsetup status --json exit_code: {}, stdout: {}",
            output.exit_code,
            if output.stdout.is_empty() { "(empty)".to_string() } else { output.stdout.clone() });

        if output.success() && !output.stdout.is_empty() {
            // Parse JSON output from drbdsetup
            match drbd_utils::parse_drbd_status(&output.stdout) {
                Ok(resources) => {
                    // Find our resource
                    if let Some(resource) = resources.iter().find(|r| r.name == profile.resource_name) {
                        Some(drbd_utils::convert_resource_status(resource))
                    } else {
                        tracing::warn!("Resource '{}' not found in drbdsetup status output", profile.resource_name);
                        None
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to parse drbdsetup JSON output: {}", e);
                    None
                }
            }
        } else {
            tracing::warn!("drbdsetup status failed or empty for resource '{}', exit_code: {}",
                profile.resource_name, output.exit_code);
            None
        }
    };

    // Determine active node from local DRBD status
    let local_hostname = gethostname::gethostname().to_string_lossy().to_string();
    let nodes = state.node_store.get_all()?;

    // Find the Primary (active) node from DRBD status
    // If local is Primary, active = local. If local is Secondary, find Primary in peers.
    let active_node_from_drbd = if let Some(ref status) = local_drbd_status {
        if status.role == "Primary" {
            Some(local_hostname.clone())
        } else {
            // Local is Secondary, find Primary in peers
            status.peers.iter()
                .find(|p| p.role == "Primary")
                .map(|p| p.name.clone())
        }
    } else {
        // DRBD status not available, try drbd-reactor as fallback
        None
    };

    // Step 2: Execute drbdadm status AND drbd-reactorctl status on the ACTIVE node
    // Track whether DRBD status is from local or remote node
    let (drbd, active_node, reactor_status_raw, drbd_from_local) = if let Some(ref active_hostname) = active_node_from_drbd {
        // Find the active node from node store
        if let Some(active_node_obj) = nodes.iter().find(|n| &n.hostname == active_hostname) {
            if active_node_obj.is_local || &active_node_obj.hostname == &local_hostname {
                // Active node is local, execute locally
                tracing::info!("Active node is local, getting status locally for {}", profile.resource_name);

                let reactor_status = DrbdReactorClient::status(Some(&profile.name)).await;
                let reactor_raw = reactor_status.as_ref().map(|(_, raw)| raw.clone()).ok();
                let active_from_reactor = reactor_status.as_ref()
                    .ok()
                    .and_then(|(statuses, _)| statuses.first())
                    .and_then(|s| s.active_node.clone());

                (local_drbd_status, active_from_reactor.or_else(|| Some(active_hostname.clone())), reactor_raw, true)
            } else {
                // Active node is remote, SSH to execute both commands there
                tracing::info!("Active node is remote {}, getting status via SSH for {}", active_hostname, profile.resource_name);
                let credential = SshCredential::Password("ignored".to_string());

                // Execute drbdsetup status --json on remote node
                let drbd_cmd = format!("drbdsetup status {} --json 2>/dev/null", profile.resource_name);
                let sudo_drbd_cmd = if active_node_obj.ssh_user != "root" {
                    format!("sudo {}", drbd_cmd)
                } else {
                    drbd_cmd
                };

                let remote_drbd = match state.ssh_manager.execute(
                    &active_node_obj.ip,
                    active_node_obj.ssh_port,
                    &active_node_obj.ssh_user,
                    &credential,
                    &sudo_drbd_cmd,
                ).await {
                    Ok(output) => {
                        if !output.stdout.is_empty() {
                            // Parse JSON output from drbdsetup
                            match drbd_utils::parse_drbd_status(&output.stdout) {
                                Ok(resources) => {
                                    if let Some(resource) = resources.iter().find(|r| r.name == profile.resource_name) {
                                        Some(drbd_utils::convert_resource_status(resource))
                                    } else {
                                        None
                                    }
                                }
                                Err(_) => None
                            }
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get drbdsetup status from {}: {}", active_hostname, e);
                        None
                    }
                };

                // Execute drbd-reactorctl status on remote node
                let reactor_cmd = format!("drbd-reactorctl status {}", profile.name);
                let sudo_reactor_cmd = if active_node_obj.ssh_user != "root" {
                    format!("sudo {}", reactor_cmd)
                } else {
                    reactor_cmd
                };

                let reactor_result = state.ssh_manager.execute(
                    &active_node_obj.ip,
                    active_node_obj.ssh_port,
                    &active_node_obj.ssh_user,
                    &credential,
                    &sudo_reactor_cmd,
                ).await;

                let (reactor_raw, active_from_reactor) = match reactor_result {
                    Ok(output) => {
                        let _statuses = drbd_reactor_utils::parser::parse_reactor_status(&output.stdout, Some(&profile.name));
                        // When fetching from remote node, don't trust parsed "this node"
                        // Use the actual remote hostname we already know is active
                        (Some(output.stdout), Some(active_hostname.clone()))
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get reactor status from {}: {}", active_hostname, e);
                        (None, Some(active_hostname.clone()))
                    }
                };

                (remote_drbd, active_from_reactor, reactor_raw, false)
            }
        } else {
            tracing::warn!("Active node {} not found in node store", active_hostname);
            (local_drbd_status, Some(active_hostname.clone()), None, true)
        }
    } else {
        // No active node from DRBD, use local status
        tracing::info!("No active node determined from DRBD, using local status");
        let reactor_status = DrbdReactorClient::status(Some(&profile.name)).await;
        let reactor_raw = reactor_status.as_ref().map(|(_, raw)| raw.clone()).ok();
        let active_from_reactor = reactor_status.as_ref()
            .ok()
            .and_then(|(statuses, _)| statuses.first())
            .and_then(|s| s.active_node.clone());

        (local_drbd_status, active_from_reactor, reactor_raw, true)
    };

    let drbd_role = drbd.as_ref().map(|d| d.role.as_str());

    // Initialize service statuses from configured services
    let mut service_statuses: Vec<ServiceStatusInfo> = profile
        .promoter
        .services
        .iter()
        .map(|name| ServiceStatusInfo {
            name: name.clone(),
            active: false,
            state: "inactive".to_string(),
            enabled: false,
            active_since: None,
        })
        .collect();

    if let Some(raw) = &reactor_status_raw {
        let parsed_services = drbd_reactor_utils::parser::parse_reactor_services(raw);
        for parsed in parsed_services {
            // Update existing or add new
            if let Some(existing) = service_statuses.iter_mut().find(|s| s.name == parsed.name) {
                existing.active = parsed.active;
                existing.state = parsed.state;
            } else {
                service_statuses.push(ServiceStatusInfo {
                    name: parsed.name,
                    active: parsed.active,
                    state: parsed.state,
                    enabled: false,
                    active_since: None,
                });
            }
        }

        // Query systemd for enabled status of each service
        // We need to query on the active node where drbd-reactor is managing services
        if let Some(active_node_name) = &active_node {
            // Find the active node from node store
            let nodes = state.node_store.get_all()?;
            let active_node_obj = nodes.iter().find(|n| &n.hostname == active_node_name);

            if let Some(node) = active_node_obj {
                if node.is_local {
                    // Active node is local, query local systemd
                    let systemd = SystemdController::new().await?;
                    for service_info in &mut service_statuses {
                        if service_info.name.starts_with("ocf.rs@") {
                            continue;
                        }
                        if let Ok(status) = systemd.status(&service_info.name).await {
                            service_info.enabled = status.is_enabled();
                        }
                    }
                } else {
                    // Active node is remote, SSH to query systemd there
                    use crate::core::SshCredential;
                    let credential = SshCredential::Password("ignored".to_string());

                    for service_info in &mut service_statuses {
                        if service_info.name.starts_with("ocf.rs@") {
                            continue;
                        }
                        // Run systemctl is-enabled on remote node
                        let cmd = format!("systemctl is-enabled {}", service_info.name);
                        let sudo_cmd = if node.ssh_user != "root" {
                            format!("sudo {}", cmd)
                        } else {
                            cmd
                        };

                        match state.ssh_manager.execute(
                            &node.ip,
                            node.ssh_port,
                            &node.ssh_user,
                            &credential,
                            &sudo_cmd,
                        ).await {
                            Ok(output) => {
                                // systemctl is-enabled returns 0 if enabled, non-zero otherwise
                                service_info.enabled = output.exit_code == 0;
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to check service {} status on {}: {}",
                                    service_info.name,
                                    node.hostname,
                                    e
                                );
                                service_info.enabled = false;
                            }
                        }
                    }
                }
            } else {
                tracing::warn!("Active node {} not found in node store", active_node_name);
            }
        } else {
            // No active node, fallback to local query
            let systemd = SystemdController::new().await?;
            for service_info in &mut service_statuses {
                if service_info.name.starts_with("ocf.rs@") {
                    continue;
                }
                if let Ok(status) = systemd.status(&service_info.name).await {
                    service_info.enabled = status.is_enabled();
                }
            }
        }
    }

    let mount_point = service_statuses
        .iter()
        .find(|s| s.name.ends_with(".mount"))
        .and_then(|s| {
            let name = s.name.strip_suffix(".mount")?;
            Some(format!("/{}", name.replace('-', "/")))
        });

    let promoter_config_path = ConfigPaths::promoter_path(&profile.name);
    let promoter_config_exists = tokio::fs::metadata(&promoter_config_path).await.is_ok();
    let systemd = SystemdController::new().await?;
    let reactor_service_status = systemd.status("drbd-reactor.service").await?;
    let config = ConfigVisibility {
        promoter_config_exists,
        promoter_config_path: promoter_config_path.clone(),
        reactor_running: reactor_service_status.is_running(),
    };

    let vip_active = if let Some(vip) = &profile.vip {
        let mut active = false;

        if promoter_config_exists {
            if let Ok(cfg) = tokio::fs::read_to_string(&promoter_config_path).await {
                if cfg.contains("ocf:heartbeat:IPaddr2") && active_node.is_some() {
                    active = true;
                }
            }
        }

        if !active && active_node.is_some() {
            active = true;
        }

        if !active {
            // Check if VIP is active on any interface
            let cmd = format!("ip addr show | grep -q '{}/'", vip.address);
            let output = run_shell_command(
                &cmd,
                &format!("Check if VIP {} is active", vip.address),
            )
            .await?;
            active = output.success();
        }

        Some(active)
    } else {
        let has_vip_service = service_statuses
            .iter()
            .any(|s| s.name.contains("vip") && s.name.contains("ocf.rs@service"));

        if has_vip_service {
            let vip_service_active = service_statuses
                .iter()
                .find(|s| s.name.contains("vip") && s.name.contains("ocf.rs@service"))
                .map(|s| s.active)
                .unwrap_or(false);
            Some(vip_service_active)
        } else {
            None
        }
    };

    let status = if active_node.is_some() {
        HaProfileStatus::Active
    } else if drbd_role == Some("Primary") {
        if service_statuses.iter().all(|s| s.active) {
            HaProfileStatus::Active
        } else {
            HaProfileStatus::Error
        }
    } else if drbd_role == Some("Secondary") {
        HaProfileStatus::Standby
    } else if drbd.is_none() {
        HaProfileStatus::Stopped
    } else {
        HaProfileStatus::Unknown
    };

    let mut profile_out = profile.clone();
    profile_out.status = status.clone();

    // Get DRBD device name from DRBD status (minor number)
    let drbd_device = if let Some(drbd_status) = &drbd {
        // Build device path from minor number: /dev/drbd{minor}
        drbd_status.minor.map(|m| format!("/dev/drbd{}", m))
    } else {
        None
    };

    // Build list of configured nodes from DRBD peers and node store
    let configured_nodes = if let Ok(nodes) = state.node_store.get_all() {
        let mut node_infos = Vec::new();
        let local_hostname = gethostname::gethostname().to_string_lossy().to_string();

        // Helper to convert DRBD role to display role
        let role_to_display = |role: &str| -> String {
            match role {
                "Primary" => "Active".to_string(),
                "Secondary" => "Standby".to_string(),
                _ => role.to_string(),
            }
        };

        if let Some(drbd_status) = &drbd {
            if drbd_from_local {
                // DRBD status from local node: add local + peers
                if let Some(local_node) = nodes.iter().find(|n| n.is_local || n.hostname == local_hostname) {
                    node_infos.push(super::types::NodeConfigInfo {
                        hostname: local_node.hostname.clone(),
                        ip: local_node.ip.clone(),
                        peer_role: Some(role_to_display(&drbd_status.role)),
                    });
                }

                // Add peer nodes from DRBD
                for peer in &drbd_status.peers {
                    if let Some(node) = nodes.iter().find(|n| n.hostname == peer.name) {
                        node_infos.push(super::types::NodeConfigInfo {
                            hostname: node.hostname.clone(),
                            ip: node.ip.clone(),
                            peer_role: Some(role_to_display(&peer.role)),
                        });
                    }
                }
            } else {
                // DRBD status from remote node: use all nodes from DRBD (local is in peers)
                // drbd_status.role is the remote node's role (Primary/Active)
                // Add the remote node first
                if let Some(remote_node) = nodes.iter().find(|n| n.hostname == active_node.as_deref().unwrap_or("")) {
                    node_infos.push(super::types::NodeConfigInfo {
                        hostname: remote_node.hostname.clone(),
                        ip: remote_node.ip.clone(),
                        peer_role: Some(role_to_display(&drbd_status.role)),
                    });
                }

                // Add all peers from DRBD (this includes the local node and other peers)
                for peer in &drbd_status.peers {
                    if let Some(node) = nodes.iter().find(|n| n.hostname == peer.name) {
                        node_infos.push(super::types::NodeConfigInfo {
                            hostname: node.hostname.clone(),
                            ip: node.ip.clone(),
                            peer_role: Some(role_to_display(&peer.role)),
                        });
                    }
                }
            }
        } else {
            // Fallback: just add local node if no DRBD status
            if let Some(local_node) = nodes.iter().find(|n| n.is_local || n.hostname == local_hostname) {
                node_infos.push(super::types::NodeConfigInfo {
                    hostname: local_node.hostname.clone(),
                    ip: local_node.ip.clone(),
                    peer_role: drbd_role.map(|r| role_to_display(r)),
                });
            }
        }

        node_infos
    } else {
        Vec::new()
    };

    Ok(Json(HaProfileDetailResponse {
        profile: profile_out,
        status,
        active_node,
        mount_point,
        drbd,
        drbd_device,
        service_statuses,
        vip_active,
        config,
        reactor_status_raw,
        configured_nodes,
    }))
}

/// GET /api/v1/ha/profiles/:id/status
pub async fn get_profile_status(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
) -> AppResult<Json<HaProfileDetailResponse>> {
    fetch_profile_details(state, id_or_name).await
}

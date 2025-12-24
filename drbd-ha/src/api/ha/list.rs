//! HA Profile list and query operations
//!
//! Handles listing profiles, fetching profile details, and status queries.

use axum::{extract::{Path, State}, Json};
use drbd_reactor_utils::DrbdReactorClient;
use drbd_utils::parse_drbdadm_status;
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
    _state: Arc<AppState>,
    id_or_name: String,
) -> AppResult<Json<HaProfileDetailResponse>> {
    use crate::models::HaProfileStatus;

    // Load profile from toml file
    let config_path = ConfigPaths::promoter_path(&id_or_name);
    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|_| crate::error::AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    let profile = create_profile_from_toml(&id_or_name, &content)
        .ok_or_else(|| crate::error::AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    let (active_node, reactor_status_raw) = {
        if let Ok((statuses, raw_output)) = DrbdReactorClient::status(Some(&profile.name)).await {
            let active = statuses.first().and_then(|s| s.active_node.clone());
            (active, Some(raw_output))
        } else {
            (None, None)
        }
    };

    let drbd = {
        let cmd = format!("drbdadm status {} 2>/dev/null", profile.resource_name);
        let output = run_shell_command(
            &cmd,
            &format!("Get drbdadm status for {}", profile.resource_name),
        )
        .await?;
        if output.success() && !output.stdout.is_empty() {
            parse_drbdadm_status(&output.stdout, &profile.resource_name)
        } else {
            None
        }
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
            let cmd = format!("ip addr show {} | grep -q '{}'", vip.interface, vip.address);
            let output = run_shell_command(
                &cmd,
                &format!(
                    "Check if VIP {} is active on interface {}",
                    vip.address, vip.interface
                ),
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

    Ok(Json(HaProfileDetailResponse {
        profile: profile_out,
        status,
        active_node,
        mount_point,
        drbd,
        service_statuses,
        vip_active,
        config,
        reactor_status_raw,
    }))
}

/// GET /api/v1/ha/profiles/:id/status
pub async fn get_profile_status(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
) -> AppResult<Json<HaProfileDetailResponse>> {
    fetch_profile_details(state, id_or_name).await
}

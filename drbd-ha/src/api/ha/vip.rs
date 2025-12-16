use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::core::{
    cluster_sync::{ClusterSync, HaSyncConfig},
    config_gen::{ConfigGenerator, ConfigPaths},
    run_shell_command, validator,
};
use crate::error::{AppError, AppResult};
use crate::models::{HaProfileStatus, VipConfig};
use crate::state::{AppState, NotificationLevel};

use super::types::{AddVipRequest, VipOperationResponse};

/// POST /api/v1/ha/profiles/:id/vip
pub async fn add_vip(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
    Json(req): Json<AddVipRequest>,
) -> AppResult<Json<VipOperationResponse>> {
    validator::validate_ip_address(&req.address)?;
    validator::validate_netmask(req.netmask)?;

    let mut profile = state
        .db
        .get_ha_profile(&id_or_name)?
        .or(state.db.get_ha_profile_by_name(&id_or_name)?)
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    if profile.vip.is_some() {
        return Err(AppError::Conflict(
            "VIP already configured for this profile. Remove existing VIP first.".to_string(),
        ));
    }

    let vip = VipConfig {
        address: req.address.clone(),
        netmask: req.netmask,
        interface: req.interface.clone(),
    };

    profile.vip = Some(vip.clone());

    let config_gen = ConfigGenerator::new()?;
    let promoter_config = ConfigGenerator::promoter_from_profile(&profile);
    let config_content = config_gen.generate_promoter(&promoter_config)?;
    let config_path = ConfigPaths::promoter_path(&profile.name);

    tokio::fs::write(&config_path, &config_content)
        .await
        .map_err(|e| AppError::Config(format!("Failed to write promoter config: {}", e)))?;

    let sync_config = HaSyncConfig {
        drbd_resource_config: None, // No DRBD config changes for VIP updates
        mount_unit: None,
        service_overrides: vec![],
        promoter_config: (config_path.clone(), config_content.clone()),
    };
    let cluster_sync = ClusterSync::new(
        state.ssh_manager.clone(),
        state.db.clone(),
        state.credentials.clone(),
    );
    if let Err(e) = cluster_sync.sync_ha_config(&sync_config).await {
        tracing::warn!("Failed to sync VIP config to remote nodes: {}", e);
    }

    state.db.update_ha_profile(&profile)?;

    tracing::info!("Added VIP {} to profile {}", req.address, profile.name);
    state.send_notification(
        NotificationLevel::Success,
        "VIP Added",
        &format!("VIP {} added to profile '{}'", req.address, profile.name),
    );

    Ok(Json(VipOperationResponse {
        message: format!(
            "VIP {} added to profile {}. Reload drbd-reactor to apply.",
            req.address, profile.name
        ),
    }))
}

/// DELETE /api/v1/ha/profiles/:id/vip
pub async fn remove_vip(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
) -> AppResult<Json<VipOperationResponse>> {
    let mut profile = state
        .db
        .get_ha_profile(&id_or_name)?
        .or(state.db.get_ha_profile_by_name(&id_or_name)?)
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    let old_vip = profile
        .vip
        .clone()
        .ok_or_else(|| AppError::Validation("No VIP configured for this profile".to_string()))?;

    if profile.status == HaProfileStatus::Active {
        let vip_cmd = format!(
            "ip addr del {}/{} dev {} 2>/dev/null || true",
            old_vip.address, old_vip.netmask, old_vip.interface
        );
        let _ = run_shell_command(
            &vip_cmd,
            &format!(
                "Remove VIP {}/{} from {}",
                old_vip.address, old_vip.netmask, old_vip.interface
            ),
        )
        .await;
    }

    profile.vip = None;

    let config_gen = ConfigGenerator::new()?;
    let promoter_config = ConfigGenerator::promoter_from_profile(&profile);
    let config_content = config_gen.generate_promoter(&promoter_config)?;
    let config_path = ConfigPaths::promoter_path(&profile.name);

    tokio::fs::write(&config_path, &config_content)
        .await
        .map_err(|e| AppError::Config(format!("Failed to write promoter config: {}", e)))?;

    let sync_config = HaSyncConfig {
        drbd_resource_config: None, // No DRBD config changes for VIP updates
        mount_unit: None,
        service_overrides: vec![],
        promoter_config: (config_path.clone(), config_content.clone()),
    };
    let cluster_sync = ClusterSync::new(
        state.ssh_manager.clone(),
        state.db.clone(),
        state.credentials.clone(),
    );
    if let Err(e) = cluster_sync.sync_ha_config(&sync_config).await {
        tracing::warn!("Failed to sync VIP removal to remote nodes: {}", e);
    }

    state.db.update_ha_profile(&profile)?;

    tracing::info!(
        "Removed VIP {} from profile {}",
        old_vip.address,
        profile.name
    );
    state.send_notification(
        NotificationLevel::Info,
        "VIP Removed",
        &format!(
            "VIP {} removed from profile '{}'",
            old_vip.address, profile.name
        ),
    );

    Ok(Json(VipOperationResponse {
        message: format!(
            "VIP {} removed from profile {}. Reload drbd-reactor to apply.",
            old_vip.address, profile.name
        ),
    }))
}

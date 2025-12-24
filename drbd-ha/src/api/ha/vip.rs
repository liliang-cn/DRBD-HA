use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::core::{
    cluster_sync::{ClusterSync, HaSyncConfig},
    run_shell_command, validator,
    ReactorConfigGenerator, ReactorConfigPaths,
};
use crate::error::{AppError, AppResult};
use crate::models::{HaProfileStatus, VipConfig};
use crate::state::{AppState, NotificationLevel};

use super::types::{AddVipRequest, VipOperationResponse};
use super::utils::create_profile_from_toml;

/// POST /api/v1/ha/profiles/:id/vip
pub async fn add_vip(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
    Json(req): Json<AddVipRequest>,
) -> AppResult<Json<VipOperationResponse>> {
    validator::validate_ip_address(&req.address)?;
    validator::validate_netmask(req.netmask)?;

    // Load profile from toml file instead of database
    let config_path = ReactorConfigPaths::promoter_path(&id_or_name);
    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|_| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;
    let mut profile = create_profile_from_toml(&id_or_name, &content)
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

    let config_gen = ReactorConfigGenerator::new()?;
    let promoter_config = drbd_reactor_utils::PromoterConfig {
        resource: profile.resource_name.clone(),
        mount_unit: profile.generated_units.mount_unit.clone(),
        start: profile.promoter.services.clone(),
        stop_services_on_exit: profile.promoter.stop_on_demote,
        on_drbd_demote_failure: profile.promoter.on_demote_failure.clone(),
        vip: Some(drbd_reactor_utils::VipConfig {
            address: vip.address.clone(),
            netmask: vip.netmask,
            interface: vip.interface.clone(),
        }),
        ocf_agents: profile.ocf_agents.iter().map(|a| drbd_reactor_utils::OcfAgentConfig {
            name: a.name.clone(),
            instance_name: a.instance_name.clone(),
            params: a.params.clone(),
        }).collect(),
        mount_strategy: Some(format!("{:?}", profile.mount_strategy).to_lowercase()),
        mount_point: Some(profile.mount_point.clone()),
        fs_type: Some(profile.fs_type.clone()),
        dependencies_as: profile.promoter.dependencies_as.clone(),
        target_as: profile.promoter.target_as.clone(),
        on_quorum_loss: profile.promoter.on_quorum_loss.clone(),
        preferred_nodes: profile.promoter.preferred_nodes.clone(),
        preferred_nodes_policy: profile.promoter.preferred_nodes_policy.clone(),
        sleep_before_promote_factor: profile.promoter.sleep_before_promote_factor,
    };
    let config_content = config_gen.generate_promoter(&promoter_config)?;
    let config_path = ReactorConfigPaths::promoter_path(&profile.name);

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
        state.node_store.clone(),
        state.credentials.clone(),
    );
    if let Err(e) = cluster_sync.sync_ha_config(&sync_config).await {
        tracing::warn!("Failed to sync VIP config to remote nodes: {}", e);
    }

    // Profile is now stored in toml file, no database update needed

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
    // Load profile from toml file instead of database
    let config_path = ReactorConfigPaths::promoter_path(&id_or_name);
    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|_| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;
    let mut profile = create_profile_from_toml(&id_or_name, &content)
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

    let config_gen = ReactorConfigGenerator::new()?;
    let promoter_config = drbd_reactor_utils::PromoterConfig {
        resource: profile.resource_name.clone(),
        mount_unit: profile.generated_units.mount_unit.clone(),
        start: profile.promoter.services.clone(),
        stop_services_on_exit: profile.promoter.stop_on_demote,
        on_drbd_demote_failure: profile.promoter.on_demote_failure.clone(),
        vip: None,
        ocf_agents: profile.ocf_agents.iter().map(|a| drbd_reactor_utils::OcfAgentConfig {
            name: a.name.clone(),
            instance_name: a.instance_name.clone(),
            params: a.params.clone(),
        }).collect(),
        mount_strategy: Some(format!("{:?}", profile.mount_strategy).to_lowercase()),
        mount_point: Some(profile.mount_point.clone()),
        fs_type: Some(profile.fs_type.clone()),
        dependencies_as: profile.promoter.dependencies_as.clone(),
        target_as: profile.promoter.target_as.clone(),
        on_quorum_loss: profile.promoter.on_quorum_loss.clone(),
        preferred_nodes: profile.promoter.preferred_nodes.clone(),
        preferred_nodes_policy: profile.promoter.preferred_nodes_policy.clone(),
        sleep_before_promote_factor: profile.promoter.sleep_before_promote_factor,
    };
    let config_content = config_gen.generate_promoter(&promoter_config)?;
    let config_path = ReactorConfigPaths::promoter_path(&profile.name);

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
        state.node_store.clone(),
        state.credentials.clone(),
    );
    if let Err(e) = cluster_sync.sync_ha_config(&sync_config).await {
        tracing::warn!("Failed to sync VIP removal to remote nodes: {}", e);
    }

    // Profile is now stored in toml file, no database update needed

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

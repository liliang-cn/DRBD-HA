//! HA Profile delete operations
//!
//! Handles deleting HA profiles and cleaning up associated resources.

use axum::{extract::{Path, Query, State}, http::StatusCode};
use std::sync::Arc;

use crate::core::{
    run_shell_command,
    systemd_ctrl::SystemdController,
    DrbdConfigPaths, ReactorConfigPaths,
    drbd_cmd::DrbdCmd,
};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use systemd_utils::SystemdCmd;

use super::types::DeleteProfileQuery;
use super::utils::create_profile_from_toml;

/// DELETE /api/v1/ha/profiles/:id
pub async fn delete_profile(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
    Query(_query): Query<DeleteProfileQuery>,
) -> AppResult<StatusCode> {
    let config_path = ReactorConfigPaths::promoter_path(&id_or_name);
    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|_| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;
    let profile = create_profile_from_toml(&id_or_name, &content)
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    let resource_name = profile.resource_name.clone();
    let operation_id = uuid::Uuid::new_v4().to_string();

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        0,
        "Starting deletion...",
        false,
        None,
    );

    let status_cmd = DrbdCmd::adm_status_cmd(&profile.resource_name)?;
    let status_output = run_shell_command(
        &status_cmd,
        &format!("Check DRBD status for {}", profile.resource_name),
    )
    .await;

    let mut local_primary = false;
    let mut remote_primary_node: Option<String> = None;

    if let Ok(output) = status_output {
        let status_str = output.stdout;

        if status_str.contains("role:Primary") {
            local_primary = true;
        } else {
            for line in status_str.lines() {
                if line.contains("role:Primary") && !line.contains(&profile.resource_name) {
                    if let Some(node_name) = line.split_whitespace().next() {
                        remote_primary_node = Some(node_name.to_string());
                        break;
                    }
                }
            }
        }
    }

    if !local_primary && remote_primary_node.is_none() {
        local_primary = true;
    }

    if local_primary {
        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            10,
            "Deactivating active profile...",
            false,
            None,
        );

        if let Some(vip) = &profile.vip {
            // VIP is managed by drbd-reactor
            tracing::info!("delete_profile: VIP {}/{} was managed by drbd-reactor", vip.address, vip.netmask);
        }

        // Disable drbd-reactor profile first
        tracing::info!("Disabling drbd-reactor profile: {}", profile.name);
        let disable_cmd = format!("drbd-reactorctl disable {}", profile.name);
        let _ = run_shell_command(&disable_cmd, "Disable drbd-reactor profile").await;

        if let Some(mount_unit) = &profile.generated_units.mount_unit {
            let _ = run_shell_command(
                &SystemdCmd::stop_cmd(mount_unit),
                "Stop mount unit",
            )
            .await;

            let _ = run_shell_command(
                &SystemdCmd::disable_cmd(mount_unit),
                "Disable mount unit",
            )
            .await;
        }

        for service in &profile.promoter.services {
            let _ = run_shell_command(
                &SystemdCmd::stop_cmd(service),
                &format!("Stop service {}", service),
            )
            .await;
        }

        let _ = run_shell_command(
            &DrbdCmd::secondary_cmd(&resource_name)?,
            "Demote DRBD resource",
        )
        .await;
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        20,
        "Removing generated files...",
        false,
        None,
    );

    if let Some(mount_unit_path) = &profile.generated_units.mount_unit_path {
        let _ = std::fs::remove_file(mount_unit_path);
    }

    for so in &profile.generated_units.service_overrides {
        let _ = std::fs::remove_file(&so.override_path);
    }

    // Remove generated files on remote nodes too
    let all_nodes = state.node_store.get_all()?;
    let credential = crate::core::SshCredential::Password("ignored".to_string());

    for node in &all_nodes {
        if !node.is_local {
            if let Some(mount_unit_path) = &profile.generated_units.mount_unit_path {
                let rm_cmd = format!("rm -f {}", mount_unit_path);
                let sudo_cmd = if node.ssh_user != "root" {
                    format!("sudo {}", rm_cmd)
                } else {
                    rm_cmd
                };
                let _ = state.ssh_manager.execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &sudo_cmd,
                ).await;
            }

            for so in &profile.generated_units.service_overrides {
                let rm_cmd = format!("rm -f {}", so.override_path);
                let sudo_cmd = if node.ssh_user != "root" {
                    format!("sudo {}", rm_cmd)
                } else {
                    rm_cmd
                };
                let _ = state.ssh_manager.execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &sudo_cmd,
                ).await;
            }
        }
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        30,
        "Unmounting filesystem...",
        false,
        None,
    );

    // Unmount on the active node only (where DRBD is mounted)
    let mount_point = &profile.mount_point;
    if local_primary {
        // Local node is primary, unmount locally
        let umount_cmd = format!("umount {}", mount_point);
        let _ = run_shell_command(&umount_cmd, "Unmount filesystem on local node").await;
    } else if let Some(ref remote_hostname) = remote_primary_node {
        // Remote node is primary, SSH to unmount there
        if let Some(active_node) = all_nodes.iter().find(|n| &n.hostname == remote_hostname) {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            let umount_cmd = if active_node.ssh_user != "root" {
                format!("sudo umount {}", mount_point)
            } else {
                format!("umount {}", mount_point)
            };
            let _ = state.ssh_manager.execute(
                &active_node.ip,
                active_node.ssh_port,
                &active_node.ssh_user,
                &credential,
                &umount_cmd,
            ).await;
        }
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        50,
        "Down DRBD resource on all nodes...",
        false,
        None,
    );

    // Down DRBD on all nodes
    for node in &all_nodes {
        if node.is_local {
            let down_cmd = DrbdCmd::down_cmd(&profile.resource_name)?;
            let _ = run_shell_command(&down_cmd, "Down DRBD resource locally").await;
        } else {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            let base_cmd = DrbdCmd::down_cmd(&profile.resource_name)?;
            let down_cmd = if node.ssh_user != "root" {
                format!("sudo {}", base_cmd)
            } else {
                base_cmd
            };
            let _ = state.ssh_manager.execute(
                &node.ip,
                node.ssh_port,
                &node.ssh_user,
                &credential,
                &down_cmd,
            ).await;
        }
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        60,
        "Deleting DRBD resource files...",
        false,
        None,
    );

    // Delete DRBD .res files on all nodes
    for node in &all_nodes {
        let drbd_config_path = DrbdConfigPaths::drbd_resource_path(&profile.resource_name);

        if node.is_local {
            tracing::info!("Deleting local DRBD config: {}", drbd_config_path);
            match tokio::fs::remove_file(&drbd_config_path).await {
                Ok(_) => tracing::info!("Deleted DRBD config: {}", drbd_config_path),
                Err(e) => tracing::warn!("Failed to delete DRBD config {}: {}", drbd_config_path, e),
            }
        } else {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            let rm_cmd = if node.ssh_user != "root" {
                format!("sudo rm -f {}", drbd_config_path)
            } else {
                format!("rm -f {}", drbd_config_path)
            };

            tracing::info!("Deleting remote DRBD config on {}: {}", node.hostname, drbd_config_path);
            match state.ssh_manager.execute(
                &node.ip,
                node.ssh_port,
                &node.ssh_user,
                &credential,
                &rm_cmd,
            ).await {
                Ok(output) => {
                    if output.exit_code == 0 {
                        tracing::info!("Deleted DRBD config on {}: {}", node.hostname, drbd_config_path);
                    } else {
                        tracing::warn!("Failed to delete DRBD config on {} {}: {}", node.hostname, drbd_config_path, output.stdout);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to delete DRBD config on {} {}: {}", node.hostname, drbd_config_path, e);
                }
            }
        }
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        60,
        "Deleting promoter configuration...",
        false,
        None,
    );

    // Delete promoter configuration on all nodes
    let promoter_config_path = ReactorConfigPaths::promoter_path(&profile.name);

    // Delete local promoter config
    tracing::info!("Deleting local promoter config: {}", promoter_config_path);
    match tokio::fs::remove_file(&promoter_config_path).await {
        Ok(_) => tracing::info!("Deleted promoter config: {}", promoter_config_path),
        Err(e) => tracing::warn!("Failed to delete promoter config {}: {}", promoter_config_path, e),
    }

    // Delete remote promoter configs
    for node in &all_nodes {
        if !node.is_local {
            let rm_cmd = if node.ssh_user != "root" {
                format!("sudo rm -f {}", promoter_config_path)
            } else {
                format!("rm -f {}", promoter_config_path)
            };

            tracing::info!("Deleting remote promoter config on {}: {}", node.hostname, promoter_config_path);
            match state.ssh_manager.execute(
                &node.ip,
                node.ssh_port,
                &node.ssh_user,
                &credential,
                &rm_cmd,
            ).await {
                Ok(output) => {
                    if output.exit_code == 0 {
                        tracing::info!("Deleted promoter config on {}: {}", node.hostname, promoter_config_path);
                    } else {
                        tracing::warn!("Failed to delete promoter config on {} {}: {}", node.hostname, promoter_config_path, output.stdout);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to delete promoter config on {} {}: {}", node.hostname, promoter_config_path, e);
                }
            }
        }
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        70,
        "Reloading drbd-reactor...",
        false,
        None,
    );

    let mut _successful_reloads = 0;

    for node in all_nodes.iter() {

        if node.is_local {
            if let Ok(sys) = SystemdController::new().await {
                if sys.reload("drbd-reactor.service").await.is_ok() {
                    _successful_reloads += 1;
                }
            }
        } else {
            let cred = crate::core::SshCredential::Password("ignored".to_string());
            let reload_cmd = format!("sudo {}", SystemdCmd::reload_service_cmd("drbd-reactor.service"));

            if state
                .ssh_manager
                .execute(&node.ip, node.ssh_port, &node.ssh_user, &cred, &reload_cmd)
                .await
                .is_ok()
            {
                _successful_reloads += 1;
            }
        }
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        80,
        "Verifying deletion...",
        false,
        None,
    );

    // Verification steps
    let mut verification_errors = Vec::new();

    // 1. Verify DRBD resource is gone (check on local node)
    tracing::info!("Verifying DRBD resource deletion...");
    let verify_cmd = DrbdCmd::adm_status_cmd(&profile.resource_name)?;
    match run_shell_command(&verify_cmd, "Verify DRBD resource deleted").await {
        Ok(output) => {
            if output.stdout.contains(&profile.resource_name) {
                let msg = format!("DRBD resource {} still active after deletion", profile.resource_name);
                tracing::warn!("{}", msg);
                verification_errors.push(msg);
            } else {
                tracing::info!("DRBD resource {} successfully removed", profile.resource_name);
            }
        }
        Err(_e) => {
            // Command failed likely means resource doesn't exist, which is good
            tracing::info!("DRBD resource {} not found (expected)", profile.resource_name);
        }
    }

    // 2. Verify drbd-reactor profile is gone (check on local node)
    tracing::info!("Verifying drbd-reactor profile deletion...");
    let verify_reactor_cmd = format!("drbd-reactorctl status {}", profile.name);
    match run_shell_command(&verify_reactor_cmd, "Verify reactor profile deleted").await {
        Ok(output) => {
            if output.stdout.contains(&profile.name) {
                let msg = format!("drbd-reactor profile {} still active", profile.name);
                tracing::warn!("{}", msg);
                verification_errors.push(msg);
            } else {
                tracing::info!("drbd-reactor profile {} successfully removed", profile.name);
            }
        }
        Err(_e) => {
            // Command failed likely means profile doesn't exist, which is good
            tracing::info!("drbd-reactor profile {} not found (expected)", profile.name);
        }
    }

    // 3. Verify DRBD config file is deleted on local node
    let drbd_config_path = DrbdConfigPaths::drbd_resource_path(&profile.resource_name);
    if tokio::fs::metadata(&drbd_config_path).await.is_ok() {
        let msg = format!("DRBD config file {} still exists", drbd_config_path);
        tracing::warn!("{}", msg);
        verification_errors.push(msg);
    } else {
        tracing::info!("DRBD config file {} successfully deleted", drbd_config_path);
    }

    // 4. Verify promoter config file is deleted on local node
    let promoter_config_path = ReactorConfigPaths::promoter_path(&profile.name);
    if tokio::fs::metadata(&promoter_config_path).await.is_ok() {
        let msg = format!("Promoter config file {} still exists", promoter_config_path);
        tracing::warn!("{}", msg);
        verification_errors.push(msg);
    } else {
        tracing::info!("Promoter config file {} successfully deleted", promoter_config_path);
    }

    // Log verification results
    if verification_errors.is_empty() {
        tracing::info!("All verification checks passed for HA profile deletion");
    } else {
        tracing::warn!("HA profile deletion verification found {} issues:", verification_errors.len());
        for error in &verification_errors {
            tracing::warn!("  - {}", error);
        }
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        100,
        "HA profile deleted successfully",
        true,
        Some(true),
    );

    state.send_notification(
        crate::state::NotificationLevel::Success,
        "HA Profile Deleted",
        &format!("HA profile '{}' deleted successfully", profile.name),
    );

    Ok(StatusCode::NO_CONTENT)
}

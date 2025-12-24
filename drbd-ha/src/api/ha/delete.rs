//! HA Profile delete operations
//!
//! Handles deleting HA profiles and cleaning up associated resources.

use axum::{extract::{Path, Query, State}, http::StatusCode};
use std::sync::Arc;

use crate::core::{
    run_shell_command,
    systemd_ctrl::{RemoteSystemdController, SystemdController},
    DrbdConfigPaths, ReactorConfigPaths,
};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

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

    let status_cmd = format!("drbdadm status {}", profile.resource_name);
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
            let vip_cmd = format!(
                "ip addr del {}/{} dev {} 2>/dev/null || true",
                vip.address, vip.netmask, vip.interface
            );
            let _ = run_shell_command(
                &vip_cmd,
                &format!("Remove VIP {} from {}", vip.address, vip.interface),
            )
            .await;
        }

        if let Some(mount_unit) = &profile.generated_units.mount_unit {
            let _ = run_shell_command(
                &format!("systemctl stop {}", mount_unit),
                "Stop mount unit",
            )
            .await;

            let _ = run_shell_command(
                &format!("systemctl disable {}", mount_unit),
                "Disable mount unit",
            )
            .await;
        }

        for service in &profile.promoter.services {
            let _ = run_shell_command(
                &format!("systemctl stop {}", service),
                &format!("Stop service {}", service),
            )
            .await;
        }

        let _ = run_shell_command(
            &format!("drbdadm secondary {}", resource_name),
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

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        30,
        "Deleting promoter configuration...",
        false,
        None,
    );

    let promoter_config_path = ReactorConfigPaths::promoter_path(&profile.name);
    let _ = tokio::fs::remove_file(&promoter_config_path).await;

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        40,
        "Removing DRBD configuration...",
        false,
        None,
    );

    let all_nodes = state.node_store.get_all()?;

    for node in &all_nodes {
        let credential = crate::core::SshCredential::Password("ignored".to_string());
        let remote_promoter_path = ReactorConfigPaths::promoter_path(&profile.name);

        if !node.is_local {
            let rm_cmd = format!("rm -f {}", remote_promoter_path);
            let _ = state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &rm_cmd,
                )
                .await;

            if let Some(mount_unit_path) = &profile.generated_units.mount_unit_path {
                let rm_cmd = format!("rm -f {}", mount_unit_path);
                let _ = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &rm_cmd,
                    )
                    .await;
            }

            for so in &profile.generated_units.service_overrides {
                let rm_cmd = format!("rm -f {}", so.override_path);
                let _ = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &rm_cmd,
                    )
                    .await;
            }
        }
    }

    let all_nodes = state.node_store.get_all()?;

    for node in &all_nodes {
        let credential = crate::core::SshCredential::Password("ignored".to_string());
        let drbd_config_path = DrbdConfigPaths::drbd_resource_path(&profile.resource_name);

        if !node.is_local {
            let rm_cmd = format!("rm -f {}", drbd_config_path);
            let _ = state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &rm_cmd,
                )
                .await;
        }
    }

    let _ = tokio::fs::remove_file(&DrbdConfigPaths::drbd_resource_path(&profile.resource_name)).await;

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        60,
        "Down DRBD resource...",
        false,
        None,
    );

    let down_cmd = format!("drbdadm down {}", profile.resource_name);
    let _ = run_shell_command(&down_cmd, "Down DRBD resource").await;

    for node in &all_nodes {
        if !node.is_local {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            let _ = state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &down_cmd,
                )
                .await;
        }
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        70,
        "Re-enabling services...",
        false,
        None,
    );

    for service in &profile.promoter.services {
        let _ = run_shell_command(
            &format!("systemctl enable {}", service),
            &format!("Enable service {}", service),
        )
        .await;
    }

    let remote_sys = RemoteSystemdController::new(state.ssh_manager.clone());
    let credential = crate::core::SshCredential::Password("ignored".to_string());

    for node in &all_nodes {
        if !node.is_local {
            for service in &profile.promoter.services {
                let _ = remote_sys
                    .enable(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        service,
                    )
                    .await;
            }
        }
    }

    if let Ok(sys) = SystemdController::new().await {
        let _ = sys.daemon_reload().await;
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        80,
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
            let reload_cmd = "sudo systemctl reload drbd-reactor.service";

            if state
                .ssh_manager
                .execute(&node.ip, node.ssh_port, &node.ssh_user, &cred, reload_cmd)
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

//! HA Profile delete operations
//!
//! Handles deleting HA profiles and cleaning up associated resources.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
};
use drbd_reactor_utils::DrbdReactorClient;
use std::sync::Arc;

use crate::core::{
    drbd_cmd::DrbdCmd, run_shell_command, service_override::ServiceOverrideGenerator,
    systemd_ctrl::SystemdController, DrbdConfigPaths, ReactorConfigPaths,
};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use systemd_utils::SystemdCmd;

use super::types::DeleteProfileQuery;
use super::utils::create_profile_from_toml;

/// DELETE /api/v1/ha/profiles/:id
#[utoipa::path(
    delete,
    path = "/api/v1/ha/profiles/{id}",
    tag = "ha",
    params(
        ("id" = String, Path, description = "Profile ID or name"),
        ("delete_resource" = Option<bool>, Query, description = "Also delete the associated DRBD resource"),
        ("delete_config_file" = Option<bool>, Query, description = "Also delete the promoter configuration file from disk")
    ),
    responses(
        (status = 204, description = "Profile deleted successfully"),
        (status = 404, description = "Profile not found")
    )
)]
pub async fn delete_profile(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
    Query(_query): Query<DeleteProfileQuery>,
) -> AppResult<StatusCode> {
    let config_path = ReactorConfigPaths::promoter_path(&id_or_name);
    let disabled_path = format!("{}.disabled", config_path);

    // Try to read from .toml (enabled) or .toml.disabled (disabled)
    let (content, _is_disabled) =
        if let Ok(content) = state.read_controller_file(&config_path).await {
            (content, false)
        } else if let Ok(content) = state.read_controller_file(&disabled_path).await {
            (content, true)
        } else {
            return Err(AppError::NotFound(format!(
                "HA profile {} not found",
                id_or_name
            )));
        };

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
            tracing::info!(
                "delete_profile: VIP {}/{} was managed by drbd-reactor",
                vip.address,
                vip.netmask
            );
        }

        // Disable drbd-reactor profile first
        tracing::info!("Disabling drbd-reactor profile: {}", profile.name);
        let disable_cmd = format!("drbd-reactorctl disable {}", profile.name);
        let _ = run_shell_command(&disable_cmd, "Disable drbd-reactor profile").await;

        if let Some(mount_unit) = &profile.generated_units.mount_unit {
            let _ = run_shell_command(&SystemdCmd::stop_cmd(mount_unit), "Stop mount unit").await;

            let _ =
                run_shell_command(&SystemdCmd::disable_cmd(mount_unit), "Disable mount unit").await;
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

    // Remove mount unit file (locally)
    if let Some(mount_unit_path) = &profile.generated_units.mount_unit_path {
        let _ = state.remove_controller_file(mount_unit_path).await;
    }

    // Remove service override files using ServiceOverrideGenerator (locally)
    for so in &profile.generated_units.service_overrides {
        let service_name = so
            .service_name
            .strip_suffix(".service")
            .unwrap_or(&so.service_name);
        if let Err(e) = ServiceOverrideGenerator::remove(service_name).await {
            tracing::warn!(
                "Failed to remove service override for {}: {}",
                service_name,
                e
            );
        }
    }

    // Remove generated files on remote nodes too
    let all_nodes = state.node_store.get_all()?;
    let credential = crate::core::SshCredential::Password("ignored".to_string());

    for node in &all_nodes {
        if !state.is_controller_node(node) {
            // Remove mount unit on remote node
            if let Some(mount_unit_path) = &profile.generated_units.mount_unit_path {
                let rm_cmd = format!("rm -f {}", mount_unit_path);
                let sudo_cmd = if node.ssh_user != "root" {
                    format!("sudo {}", rm_cmd)
                } else {
                    rm_cmd
                };
                let _ = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &sudo_cmd,
                    )
                    .await;
            }

            // Remove service override files on remote node
            for so in &profile.generated_units.service_overrides {
                let service_name = so
                    .service_name
                    .strip_suffix(".service")
                    .unwrap_or(&so.service_name);

                // Remove /etc/systemd/system/{service}.d/ha-override.conf
                let override_dir = format!("/etc/systemd/system/{}.service.d", service_name);
                let override_path = format!("{}/ha-override.conf", override_dir);
                let rm_override = if node.ssh_user != "root" {
                    format!("sudo rm -f {}", override_path)
                } else {
                    format!("rm -f {}", override_path)
                };
                let _ = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &rm_override,
                    )
                    .await;

                // Try to remove the directory if it's empty
                let rmdir_cmd = if node.ssh_user != "root" {
                    format!("sudo rmdir {} 2>/dev/null || true", override_dir)
                } else {
                    format!("rmdir {} 2>/dev/null || true", override_dir)
                };
                let _ = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &rmdir_cmd,
                    )
                    .await;

                // Remove /run/systemd/system/{service}.d/reactor.conf
                let run_override_dir = format!("/run/systemd/system/{}.service.d", service_name);
                let run_override_path = format!("{}/reactor.conf", run_override_dir);
                let rm_runtime = if node.ssh_user != "root" {
                    format!("sudo rm -f {}", run_override_path)
                } else {
                    format!("rm -f {}", run_override_path)
                };
                let _ = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &rm_runtime,
                    )
                    .await;

                // Try to remove the runtime directory if it's empty
                let rmdir_runtime = if node.ssh_user != "root" {
                    format!("sudo rmdir {} 2>/dev/null || true", run_override_dir)
                } else {
                    format!("rmdir {} 2>/dev/null || true", run_override_dir)
                };
                let _ = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &rmdir_runtime,
                    )
                    .await;
            }

            // Reload systemd daemon on remote node
            let daemon_reload_cmd = if node.ssh_user != "root" {
                "sudo systemctl daemon-reload"
            } else {
                "systemctl daemon-reload"
            };
            let _ = state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    daemon_reload_cmd,
                )
                .await;
        }
    }

    // Reload systemd daemon locally
    let _ = run_shell_command("systemctl daemon-reload", "Reload systemd daemon").await;

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
            let _ = state
                .ssh_manager
                .execute(
                    &active_node.ip,
                    active_node.ssh_port,
                    &active_node.ssh_user,
                    &credential,
                    &umount_cmd,
                )
                .await;
        }
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        50,
        "Stopping DRBD resource on all nodes...",
        false,
        None,
    );

    // Step 1: Make all nodes secondary and disconnect
    tracing::info!("Making all nodes secondary and disconnecting...");
    for node in &all_nodes {
        let secondary_cmd = DrbdCmd::secondary_cmd(&profile.resource_name)?;
        let disconnect_cmd = DrbdCmd::disconnect_cmd(&profile.resource_name)?;

        if state.is_controller_node(node) {
            // Local node - make secondary
            if let Ok(output) = run_shell_command(&secondary_cmd, "Make DRBD secondary").await {
                if output.success() {
                    tracing::info!("Made DRBD secondary on local node");
                } else {
                    tracing::debug!(
                        "Secondary command output (may be already secondary): {}",
                        output.stderr
                    );
                }
            }

            // Disconnect
            if let Ok(output) = run_shell_command(&disconnect_cmd, "Disconnect DRBD").await {
                if output.success() {
                    tracing::info!("Disconnected DRBD on local node");
                }
            }
        } else {
            let credential = crate::core::SshCredential::Password("ignored".to_string());

            // Make secondary on remote node
            let remote_secondary = if node.ssh_user != "root" {
                format!("sudo {}", secondary_cmd)
            } else {
                secondary_cmd.clone()
            };

            if let Ok(output) = state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &remote_secondary,
                )
                .await
            {
                if output.success() {
                    tracing::info!("Made DRBD secondary on {}", node.hostname);
                }
            }

            // Disconnect on remote node
            let remote_disconnect = if node.ssh_user != "root" {
                format!("sudo {}", disconnect_cmd)
            } else {
                disconnect_cmd.clone()
            };

            if let Ok(output) = state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &remote_disconnect,
                )
                .await
            {
                if output.success() {
                    tracing::info!("Disconnected DRBD on {}", node.hostname);
                }
            }
        }
    }

    // Wait for disconnection to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        55,
        "Down DRBD resource on all nodes...",
        false,
        None,
    );

    // Step 2: Down DRBD on all nodes
    let mut down_failed_nodes = Vec::new();
    for node in &all_nodes {
        if state.is_controller_node(node) {
            let down_cmd = DrbdCmd::down_cmd(&profile.resource_name)?;
            match run_shell_command(&down_cmd, "Down DRBD resource locally").await {
                Ok(output) => {
                    if !output.success() {
                        tracing::warn!("Failed to down DRBD locally: {}", output.stderr);
                        down_failed_nodes.push((node.hostname.clone(), output.stderr));
                    } else {
                        tracing::info!("DRBD resource downed successfully on local node");
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to down DRBD locally: {}", e);
                    down_failed_nodes.push((node.hostname.clone(), e.to_string()));
                }
            }
        } else {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            let base_cmd = DrbdCmd::down_cmd(&profile.resource_name)?;
            let down_cmd = if node.ssh_user != "root" {
                format!("sudo {}", base_cmd)
            } else {
                base_cmd
            };
            match state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &down_cmd,
                )
                .await
            {
                Ok(output) => {
                    if output.exit_code != 0 {
                        tracing::warn!(
                            "Failed to down DRBD on {}: {}",
                            node.hostname,
                            output.stderr
                        );
                        down_failed_nodes.push((node.hostname.clone(), output.stderr));
                    } else {
                        tracing::info!("DRBD resource downed successfully on {}", node.hostname);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to down DRBD on {}: {}", node.hostname, e);
                    down_failed_nodes.push((node.hostname.clone(), e.to_string()));
                }
            }
        }
    }

    // Wait a moment for DRBD to fully shut down
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Log and continue even if some nodes failed (we still want to clean up config files)
    if !down_failed_nodes.is_empty() {
        tracing::warn!(
            "DRBD down failed on {} node(s): {:?}",
            down_failed_nodes.len(),
            down_failed_nodes
        );
        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            58,
            &format!(
                "Warning: DRBD down failed on {} node(s), continuing cleanup...",
                down_failed_nodes.len()
            ),
            false,
            None,
        );
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        60,
        "Wiping DRBD metadata...",
        false,
        None,
    );

    // Wipe DRBD metadata on all nodes BEFORE deleting .res files
    // This is important because wipe-md needs the .res file to know which device to wipe
    tracing::info!(
        "Wiping DRBD metadata for resource: {}",
        profile.resource_name
    );
    for node in &all_nodes {
        if state.is_controller_node(node) {
            // Wipe metadata locally - must be done while .res file still exists
            let wipe_cmd = DrbdCmd::wipe_md_cmd(&profile.resource_name)?;
            // Add --force flag to skip confirmation
            let wipe_cmd_force = format!("{} --force", wipe_cmd);
            match run_shell_command(&wipe_cmd_force, "Wipe DRBD metadata locally").await {
                Ok(output) => {
                    if output.success() {
                        tracing::info!("Wiped DRBD metadata on local node");
                    } else {
                        tracing::warn!("Could not wipe metadata locally: {}", output.stderr);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to wipe metadata locally: {}", e);
                }
            }
        } else {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            let base_wipe = DrbdCmd::wipe_md_cmd(&profile.resource_name)?;
            let wipe_cmd_force = format!("{} --force", base_wipe);
            let remote_cmd = if node.ssh_user != "root" {
                format!("sudo {}", wipe_cmd_force)
            } else {
                wipe_cmd_force
            };

            match state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &remote_cmd,
                )
                .await
            {
                Ok(output) => {
                    if output.success() {
                        tracing::info!("Wiped DRBD metadata on {}", node.hostname);
                    } else {
                        tracing::warn!(
                            "Could not wipe metadata on {}: {}",
                            node.hostname,
                            output.stderr
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to wipe metadata on {}: {}", node.hostname, e);
                }
            }
        }
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        65,
        "Deleting DRBD resource files...",
        false,
        None,
    );

    // Delete DRBD .res files on all nodes (AFTER wiping metadata)
    for node in &all_nodes {
        let drbd_config_path = DrbdConfigPaths::drbd_resource_path(&profile.resource_name);

        if state.is_controller_node(node) {
            tracing::info!("Deleting local DRBD config: {}", drbd_config_path);
            match state.remove_controller_file(&drbd_config_path).await {
                Ok(_) => tracing::info!("Deleted DRBD config: {}", drbd_config_path),
                Err(e) => {
                    tracing::warn!("Failed to delete DRBD config {}: {}", drbd_config_path, e)
                }
            }
        } else {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            let rm_cmd = if node.ssh_user != "root" {
                format!("sudo rm -f {}", drbd_config_path)
            } else {
                format!("rm -f {}", drbd_config_path)
            };

            tracing::info!(
                "Deleting remote DRBD config on {}: {}",
                node.hostname,
                drbd_config_path
            );
            match state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &rm_cmd,
                )
                .await
            {
                Ok(output) => {
                    if output.exit_code == 0 {
                        tracing::info!(
                            "Deleted DRBD config on {}: {}",
                            node.hostname,
                            drbd_config_path
                        );
                    } else {
                        tracing::warn!(
                            "Failed to delete DRBD config on {} {}: {}",
                            node.hostname,
                            drbd_config_path,
                            output.stdout
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to delete DRBD config on {} {}: {}",
                        node.hostname,
                        drbd_config_path,
                        e
                    );
                }
            }
        }
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        70,
        "Deleting promoter configuration...",
        false,
        None,
    );

    // Delete promoter configuration on all nodes (both .toml and .toml.disabled)
    let promoter_config_path = ReactorConfigPaths::promoter_path(&profile.name);
    let promoter_disabled_path = format!("{}.disabled", promoter_config_path);

    // Delete local promoter config (.toml)
    tracing::info!("Deleting local promoter config: {}", promoter_config_path);
    match state.remove_controller_file(&promoter_config_path).await {
        Ok(_) => tracing::info!("Deleted promoter config: {}", promoter_config_path),
        Err(e) => tracing::warn!(
            "Failed to delete promoter config {}: {}",
            promoter_config_path,
            e
        ),
    }

    // Delete local promoter disabled config (.toml.disabled)
    tracing::info!(
        "Deleting local promoter disabled config: {}",
        promoter_disabled_path
    );
    match state.remove_controller_file(&promoter_disabled_path).await {
        Ok(_) => tracing::info!(
            "Deleted promoter disabled config: {}",
            promoter_disabled_path
        ),
        Err(e) => tracing::debug!(
            "Failed to delete promoter disabled config {} (may not exist): {}",
            promoter_disabled_path,
            e
        ),
    }

    // Delete remote promoter configs
    for node in &all_nodes {
        if !state.is_controller_node(node) {
            // Delete both .toml and .toml.disabled files
            let rm_cmd = if node.ssh_user != "root" {
                format!(
                    "sudo rm -f {} {}",
                    promoter_config_path, promoter_disabled_path
                )
            } else {
                format!("rm -f {} {}", promoter_config_path, promoter_disabled_path)
            };

            tracing::info!(
                "Deleting remote promoter config on {}: {}",
                node.hostname,
                promoter_config_path
            );
            match state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &rm_cmd,
                )
                .await
            {
                Ok(output) => {
                    if output.exit_code == 0 {
                        tracing::info!(
                            "Deleted promoter config on {}: {}",
                            node.hostname,
                            promoter_config_path
                        );
                    } else {
                        tracing::warn!(
                            "Failed to delete promoter config on {} {}: {}",
                            node.hostname,
                            promoter_config_path,
                            output.stdout
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to delete promoter config on {} {}: {}",
                        node.hostname,
                        promoter_config_path,
                        e
                    );
                }
            }
        }
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        75,
        "Reloading drbd-reactor...",
        false,
        None,
    );

    let mut _successful_reloads = 0;

    for node in all_nodes.iter() {
        if state.is_controller_node(node) {
            if let Ok(sys) = SystemdController::new().await {
                if sys.reload("drbd-reactor.service").await.is_ok() {
                    _successful_reloads += 1;
                }
            }
        } else {
            let cred = crate::core::SshCredential::Password("ignored".to_string());
            let reload_cmd = format!(
                "sudo {}",
                SystemdCmd::reload_service_cmd("drbd-reactor.service")
            );

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
                let msg = format!(
                    "DRBD resource {} still active after deletion",
                    profile.resource_name
                );
                tracing::warn!("{}", msg);
                verification_errors.push(msg);
            } else {
                tracing::info!(
                    "DRBD resource {} successfully removed",
                    profile.resource_name
                );
            }
        }
        Err(_e) => {
            // Command failed likely means resource doesn't exist, which is good
            tracing::info!(
                "DRBD resource {} not found (expected)",
                profile.resource_name
            );
        }
    }

    // 2. Verify drbd-reactor profile is gone (check on local node)
    tracing::info!("Verifying drbd-reactor profile deletion...");
    match DrbdReactorClient::status(Some(&profile.name), None).await {
        Ok((statuses, _)) if statuses.is_empty() => {
            tracing::info!("drbd-reactor profile {} successfully removed", profile.name);
        }
        Ok(_) => {
            let msg = format!("drbd-reactor profile {} still active", profile.name);
            tracing::warn!("{}", msg);
            verification_errors.push(msg);
        }
        Err(_e) => {
            tracing::info!("drbd-reactor profile {} not found (expected)", profile.name);
        }
    }

    // 3. Verify DRBD config file is deleted on local node
    let drbd_config_path = DrbdConfigPaths::drbd_resource_path(&profile.resource_name);
    if state
        .controller_file_exists(&drbd_config_path)
        .await
        .unwrap_or(false)
    {
        let msg = format!("DRBD config file {} still exists", drbd_config_path);
        tracing::warn!("{}", msg);
        verification_errors.push(msg);
    } else {
        tracing::info!("DRBD config file {} successfully deleted", drbd_config_path);
    }

    // 4. Verify promoter config file is deleted on local node (check both .toml and .toml.disabled)
    let promoter_config_path = ReactorConfigPaths::promoter_path(&profile.name);
    let promoter_disabled_path = format!("{}.disabled", promoter_config_path);

    if state
        .controller_file_exists(&promoter_config_path)
        .await
        .unwrap_or(false)
    {
        let msg = format!("Promoter config file {} still exists", promoter_config_path);
        tracing::warn!("{}", msg);
        verification_errors.push(msg);
    } else {
        tracing::info!(
            "Promoter config file {} successfully deleted",
            promoter_config_path
        );
    }

    if state
        .controller_file_exists(&promoter_disabled_path)
        .await
        .unwrap_or(false)
    {
        let msg = format!(
            "Promoter disabled config file {} still exists",
            promoter_disabled_path
        );
        tracing::warn!("{}", msg);
        verification_errors.push(msg);
    } else {
        tracing::debug!(
            "Promoter disabled config file {} successfully deleted",
            promoter_disabled_path
        );
    }

    // Log verification results
    if verification_errors.is_empty() {
        tracing::info!("All verification checks passed for HA profile deletion");
    } else {
        tracing::warn!(
            "HA profile deletion verification found {} issues:",
            verification_errors.len()
        );
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

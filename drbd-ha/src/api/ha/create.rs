//! HA Profile create operations
//!
//! Handles creating new HA profiles with DRBD resources and configurations.

use axum::{extract::State, http::StatusCode, Json};
use drbd_migration::{DataMigration, MigrationConfig};
use std::sync::Arc;
use tracing::info;

use crate::core::{
    cluster_sync::{ClusterSync, HaSyncConfig},
    mount_unit::MountUnitGenerator,
    run_shell_command,
    service_override::ServiceOverrideGenerator,
    systemd_ctrl::{RemoteSystemdController, SystemdController},
    validator, LvmProvider, ServiceInitFactory, StorageProvider, ZfsProvider,
    DrbdConfigGenerator, DrbdConfigPaths, ReactorConfigGenerator, ReactorConfigPaths, NodeConfig, ResourceConfig,
    drbd_cmd::DrbdCmd,
};
use systemd_utils::SystemdCmd;
use crate::error::{AppError, AppResult};
use crate::models::{
    CreateHaProfileRequest, GeneratedUnits, HaProfile, HaProfileStatus, HaType, Node,
    PromoterSettings, ServiceOverride,
};
use crate::state::{AppState, NotificationLevel};

use super::types::HaProfileCreateResponse;

/// Get the actual device name for a DRBD resource from its configuration file
async fn get_drbd_device_for_resource(resource_name: &str, state: &AppState) -> Option<String> {
    let config_path = state.drbd_resource_path(resource_name);

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

/// POST /api/v1/ha/profiles
pub async fn create_profile(
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<CreateHaProfileRequest>,
) -> AppResult<(StatusCode, Json<HaProfileCreateResponse>)> {
    let operation_id = uuid::Uuid::new_v4().to_string();
    let mut generated_units = GeneratedUnits::default();
    let migration_result = None;

    state.send_progress(
        &operation_id,
        "create_ha_profile",
        Some(&req.name),
        0,
        "Validating inputs...",
        false,
        None,
    );

    validator::validate_resource_name(&req.resource_name)?;
    validator::validate_mount_point(&req.mount_point)?;
    validator::validate_fs_type(&req.fs_type)?;

    for service in &req.services {
        validator::validate_service_name(service)?;
    }

    if let Some(vip) = &req.vip {
        validator::validate_ip_address(&vip.address)?;
        validator::validate_netmask(vip.netmask)?;
    }

    let config_path = ReactorConfigPaths::promoter_path(&req.name);
    if tokio::fs::try_exists(&config_path).await.unwrap_or(false) {
        return Err(AppError::AlreadyExists(format!(
            "HA profile with name {} already exists",
            req.name
        )));
    }

    // Use the profile name as the ID (instead of random UUID)
    // This ensures the ID matches the config file name
    let profile_id = req.name.clone();
    let all_nodes = state.node_store.get_all()?;

    let drbd_port = if let Some(p) = req.drbd_port {
        p
    } else {
        super::utils::find_next_free_drbd_port(&state).await?
    };

    let drbd_minor = if let Some(m) = req.drbd_minor {
        m
    } else {
        super::utils::find_next_free_drbd_minor(&state).await?
    };

    let mut manual_filesystem_agent_present = false;
    for agent in &mut req.ocf_agents {
        if agent.name == "ocf:heartbeat:Filesystem" {
            manual_filesystem_agent_present = true;

            let device_missing_or_empty = !agent.params.contains_key("device")
                || agent
                    .params
                    .get("device")
                    .is_some_and(|s| s.is_empty());

            if device_missing_or_empty && !agent.params.contains_key("device") {
                match get_drbd_device_for_resource(&req.resource_name, &state).await {
                    Some(device) => {
                        agent.params.insert("device".to_string(), device);
                    }
                    None => {
                        return Err(AppError::Validation(format!(
                            "DRBD resource '{}' not found. Cannot determine device path for Filesystem agent.",
                            req.resource_name
                        )));
                    }
                }
            }

            if !agent.params.contains_key("directory")
                || agent
                    .params
                    .get("directory")
                    .map(|s| s.is_empty())
                    .unwrap_or(true)
            {
                agent
                    .params
                    .insert("directory".to_string(), req.mount_point.clone());
            }

            if !agent.params.contains_key("fstype")
                || agent
                    .params
                    .get("fstype")
                    .map(|s| s.is_empty())
                    .unwrap_or(true)
            {
                agent
                    .params
                    .insert("fstype".to_string(), req.fs_type.clone());
            }
        }
    }

    validator::validate_ocf_agents(&req.ocf_agents)?;

    if let (Some(pool_id), Some(volume_size_gb)) = (&req.lvm_pool_id, &req.lvm_volume_size_gb) {
        state.send_progress(
            &operation_id,
            "create_ha_profile",
            Some(&req.name),
            10,
            "Creating LVM volume...",
            false,
            None,
        );

        let vg_info = crate::core::get_vg_info(pool_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Storage pool '{}' not found", pool_id)))?;

        let storage_pool_name = vg_info.name;
        let mut node_lvm_paths: Vec<(Node, String)> = Vec::new();

        for node in &all_nodes {
            let current_vg_name = storage_pool_name.clone();
            let lv_name = format!("drbd-ha-lv-{}", req.resource_name);

            let lvm_provider = if node.is_local {
                LvmProvider::new_local(current_vg_name.clone())
            } else {
                let credential = crate::core::SshCredential::Password("ignored".to_string());
                LvmProvider::new_remote(
                    current_vg_name.clone(),
                    state.ssh_manager.clone(),
                    node.ip.clone(),
                    node.ssh_port,
                    node.ssh_user.clone(),
                    credential,
                )
            };

            let device_path = lvm_provider
                .create_volume(&lv_name, *volume_size_gb)
                .await
                .or_else(|e| {
                    if e.to_string().contains("already exists") {
                        tracing::warn!(
                            "Volume {} already exists on node {}, reusing it",
                            lv_name,
                            node.hostname
                        );
                        Ok(format!("/dev/{}/{}", current_vg_name, lv_name))
                    } else {
                        Err(e)
                    }
                })?;

            node_lvm_paths.push((node.clone(), device_path));
        }

        let mut node_configs = Vec::new();
        for (i, (node, disk)) in node_lvm_paths.iter().enumerate() {
            node_configs.push(NodeConfig {
                hostname: node.hostname.clone(),
                ip: node.ip.clone(),
                disk: disk.clone(),
                node_id: i as u32,
            });
        }

        let config_gen = DrbdConfigGenerator::new()?;

        // DRBD requires device name to match minor number (e.g., /dev/drbd10 minor 10)
        let resource_config = ResourceConfig {
            name: req.resource_name.clone(),
            port: drbd_port,
            minor: drbd_minor,
            device: format!("/dev/drbd{}", drbd_minor),
            nodes: node_configs,
            auto_promote: false,
            ..Default::default()
        };

        let config_content = config_gen.generate_drbd_resource(&resource_config)?;
        let config_path = DrbdConfigPaths::drbd_resource_path(&req.resource_name);

        tokio::fs::write(&config_path, &config_content)
            .await
            .map_err(|e| AppError::Config(format!("Failed to write DRBD config: {}", e)))?;

        generated_units.drbd_config_path = Some(config_path.clone());

        let remote_nodes: Vec<_> = all_nodes.iter().filter(|n| !n.is_local).collect();
        for node in &remote_nodes {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            let state_for_write = state.clone();

            state_for_write
                .ssh_manager
                .write_file(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &config_path,
                    &config_content,
                )
                .await?;

            let verification_config = drbd_utils::VerificationConfig {
                max_attempts: 3,
                retry_delay_secs: 2,
                continue_on_failure: true,
            };

            let node_ip = node.ip.clone();
            let node_port = node.ssh_port;
            let node_user = node.ssh_user.clone();
            let state_for_exec = state.clone();
            let ssh_executor = move |cmd: String| async move {
                match state_for_exec
                    .ssh_manager
                    .execute(&node_ip, node_port, &node_user, &credential, &cmd)
                    .await
                {
                    Ok(output) => Ok(output.stdout),
                    Err(e) => Err(shell_cmd::error::ShellError::Execution(e.to_string())),
                }
            };

            match drbd_utils::DrbdVerifier::verify_remote_drbd_status(
                &req.resource_name,
                ssh_executor,
                verification_config,
            )
            .await
            {
                Ok(result) => {
                    if result.success {
                        tracing::info!(
                            "✓ DRBD config verified on remote node {}: {} (attempts: {})",
                            node.hostname,
                            req.resource_name,
                            result.attempts
                        );
                    } else {
                        tracing::warn!(
                            "⚠ DRBD verification failed on remote node {} after {} attempts: {}",
                            node.hostname,
                            result.attempts,
                            result.details.status_info
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "⚠ Failed to verify DRBD status on remote node {}: {}",
                        node.hostname,
                        e
                    );
                }
            }
        }

        let create_md_cmd = DrbdCmd::create_md_cmd(&req.resource_name)?;
        let up_cmd = DrbdCmd::up_cmd(&req.resource_name)?;

        run_shell_command(&create_md_cmd, "Create local metadata").await?;
        run_shell_command(&up_cmd, "Up local resource").await?;

        for node in &remote_nodes {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &create_md_cmd,
                )
                .await?;
            state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &up_cmd,
                )
                .await?;
        }
    }
    else if let (Some(pool_id), Some(volume_size_gb)) =
        (&req.zfs_pool_id, &req.zfs_volume_size_gb)
    {
        state.send_progress(
            &operation_id,
            "create_ha_profile",
            Some(&req.name),
            10,
            "Creating ZFS volume...",
            false,
            None,
        );

        let zpool_name = pool_id.clone();
        let mut node_zfs_paths: Vec<(Node, String)> = Vec::new();

        for node in &all_nodes {
            let current_pool_name = zpool_name.clone();
            let zvol_name = format!("drbd-ha-zvol-{}", req.resource_name);

            let device_path: String = if node.is_local {
                let provider = ZfsProvider::new_local(current_pool_name.clone());
                provider
                    .create_volume(&zvol_name, *volume_size_gb)
                    .await
                    .or_else(|e: anyhow::Error| {
                        if e.to_string().contains("already exists") {
                            tracing::warn!(
                                "ZFS volume {} already exists on node {}, reusing it",
                                zvol_name,
                                node.hostname
                            );
                            Ok(format!("/dev/zvol/{}/{}", current_pool_name, zvol_name))
                        } else {
                            Err(e)
                        }
                    })?
            } else {
                let credential = crate::core::SshCredential::Password("ignored".to_string());
                let provider = ZfsProvider::new_remote(
                    current_pool_name.clone(),
                    state.ssh_manager.clone(),
                    node.ip.clone(),
                    node.ssh_port,
                    node.ssh_user.clone(),
                    credential,
                );
                provider
                    .create_volume(&zvol_name, *volume_size_gb)
                    .await
                    .or_else(|e: anyhow::Error| {
                        if e.to_string().contains("already exists") {
                            tracing::warn!(
                                "ZFS volume {} already exists on node {}, reusing it",
                                zvol_name,
                                node.hostname
                            );
                            Ok(format!("/dev/zvol/{}/{}", current_pool_name, zvol_name))
                        } else {
                            Err(e)
                        }
                    })?
            };

            node_zfs_paths.push((node.clone(), device_path));
        }

        let mut node_configs = Vec::new();
        for (i, (node, disk)) in node_zfs_paths.iter().enumerate() {
            node_configs.push(NodeConfig {
                hostname: node.hostname.clone(),
                ip: node.ip.clone(),
                disk: disk.clone(),
                node_id: i as u32,
            });
        }

        let config_gen = DrbdConfigGenerator::new()?;

        // DRBD requires device name to match minor number (e.g., /dev/drbd10 minor 10)
        let resource_config = ResourceConfig {
            name: req.resource_name.clone(),
            port: drbd_port,
            minor: drbd_minor,
            device: format!("/dev/drbd{}", drbd_minor),
            nodes: node_configs,
            auto_promote: false,
            ..Default::default()
        };

        let config_content = config_gen.generate_drbd_resource(&resource_config)?;
        let config_path = DrbdConfigPaths::drbd_resource_path(&req.resource_name);

        let config_dir = std::path::Path::new(&config_path).parent().unwrap();
        if !config_dir.exists() {
            tokio::fs::create_dir_all(config_dir)
                .await
                .map_err(|e| AppError::Config(format!("Failed to create config dir: {}", e)))?;
        }

        tokio::fs::write(&config_path, &config_content)
            .await
            .map_err(|e| AppError::Config(format!("Failed to write DRBD config: {}", e)))?;

        generated_units.drbd_config_path = Some(config_path.clone());

        let remote_nodes: Vec<_> = all_nodes.iter().filter(|n| !n.is_local).collect();
        for node in &remote_nodes {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            let state_for_write = state.clone();

            state_for_write
                .ssh_manager
                .write_file(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &config_path,
                    &config_content,
                )
                .await?;

            let verification_config = drbd_utils::VerificationConfig {
                max_attempts: 3,
                retry_delay_secs: 2,
                continue_on_failure: true,
            };

            let node_ip = node.ip.clone();
            let node_port = node.ssh_port;
            let node_user = node.ssh_user.clone();
            let state_for_exec = state.clone();
            let ssh_executor = move |cmd: String| async move {
                match state_for_exec
                    .ssh_manager
                    .execute(&node_ip, node_port, &node_user, &credential, &cmd)
                    .await
                {
                    Ok(output) => Ok(output.stdout),
                    Err(e) => Err(shell_cmd::error::ShellError::Execution(e.to_string())),
                }
            };

            match drbd_utils::DrbdVerifier::verify_remote_drbd_status(
                &req.resource_name,
                ssh_executor,
                verification_config,
            )
            .await
            {
                Ok(result) => {
                    if result.success {
                        tracing::info!(
                            "✓ DRBD config verified on remote node {}: {} (attempts: {})",
                            node.hostname,
                            req.resource_name,
                            result.attempts
                        );
                    } else {
                        tracing::warn!(
                            "⚠ DRBD verification failed on remote node {} after {} attempts: {}",
                            node.hostname,
                            result.attempts,
                            result.details.status_info
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "⚠ Failed to verify DRBD status on remote node {}: {}",
                        node.hostname,
                        e
                    );
                }
            }
        }

        let create_md_cmd = DrbdCmd::create_md_cmd(&req.resource_name)?;
        let up_cmd = DrbdCmd::up_cmd(&req.resource_name)?;

        run_shell_command(&create_md_cmd, "Create local metadata").await?;
        run_shell_command(&up_cmd, "Up local resource").await?;

        for node in &remote_nodes {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &create_md_cmd,
                )
                .await?;
            state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &up_cmd,
                )
                .await?;
        }
    }

    let mut messages = Vec::new();
    let extra_sync_files: Vec<(String, String)> = Vec::new();

    if req.ha_type == HaType::Generic {
        if !manual_filesystem_agent_present
            && req.mount_strategy != crate::models::MountStrategy::Ocf
        {
            state.send_progress(
                &operation_id,
                "create_ha_profile",
                Some(&req.name),
                20,
                "Generating systemd mount unit...",
                false,
                None,
            );
            let mount_info =
                MountUnitGenerator::generate(&req.resource_name, &req.mount_point, &req.fs_type)
                    .await?;

            generated_units.mount_unit = Some(mount_info.unit_name.clone());
            generated_units.mount_unit_path = Some(mount_info.unit_path.clone());
            generated_units.drbd_device = Some(mount_info.device_path.clone());
            messages.push(format!("Generated mount unit: {}", mount_info.unit_name));
        } else if manual_filesystem_agent_present {
            tracing::info!(
                "Manual Filesystem OCF agent detected, skipping systemd mount unit generation"
            );
            match get_drbd_device_for_resource(&req.resource_name, &state).await {
                Some(device) => {
                    generated_units.drbd_device = Some(device);
                }
                None => {
                    return Err(AppError::Validation(format!(
                        "DRBD resource '{}' not found. Cannot determine device path.",
                        req.resource_name
                    )));
                }
            }
        }
    }

    let _ocf_exportfs_cache: Option<String> = None;

    let migration_performed = if req.ha_type == HaType::Generic {
        if let Some(ref migration_opts) = req.migration {
            migration_opts.migrate_data
        } else {
            false
        }
    } else {
        false
    };

    match req.ha_type {
        HaType::Generic => {
            if (req.lvm_pool_id.is_some() || req.zfs_pool_id.is_some()) && !migration_performed {
                state.send_progress(
                    &operation_id,
                    "create_ha_profile",
                    Some(&req.name),
                    35,
                    "Initializing service storage...",
                    false,
                    None,
                );

                run_shell_command(
                    &DrbdCmd::primary_cmd(&req.resource_name, false)?,
                    "Promote for setup",
                )
                .await?;

                let drbd_device = get_drbd_device_for_resource(&req.resource_name, &state).await
                    .ok_or_else(|| AppError::Validation(format!(
                        "DRBD resource '{}' not found. Cannot create filesystem on non-existent device.",
                        req.resource_name
                    )))?;
                let mkfs_cmd = format!("mkfs.{} {}", req.fs_type, drbd_device);
                run_shell_command(&mkfs_cmd, "Create filesystem").await?;

                run_shell_command(
                    &format!("mkdir -p {}", req.mount_point),
                    "Create mount point",
                )
                .await?;
                run_shell_command(
                    &format!("mount {} {}", drbd_device, req.mount_point),
                    "Mount for setup",
                )
                .await?;

                if req.init_service {
                    for service in &req.services {
                        if let Some(initializer) = ServiceInitFactory::detect(service) {
                            state.send_progress(
                                &operation_id,
                                "create_ha_profile",
                                Some(&req.name),
                                37,
                                &format!("Initializing data for {}", service),
                                false,
                                None,
                            );
                            if let Err(e) = initializer.initialize(&req.mount_point).await {
                                let _ = run_shell_command(
                                    &format!("umount {}", req.mount_point),
                                    "Cleanup umount",
                                )
                                .await;
                                let _ = run_shell_command(
                                    &DrbdCmd::secondary_cmd(&req.resource_name)?,
                                    "Cleanup secondary",
                                )
                                .await;
                                return Err(e);
                            }
                            messages.push(format!("Initialized data for {}", service));
                        }
                    }
                }

                run_shell_command(
                    &format!("umount {}", req.mount_point),
                    "Unmount after setup",
                )
                .await?;
                run_shell_command(
                    &DrbdCmd::secondary_cmd(&req.resource_name)?,
                    "Demote after setup",
                )
                .await?;
            } else if (req.lvm_pool_id.is_some() || req.zfs_pool_id.is_some())
                && migration_performed
            {
                tracing::info!(
                    "Skipping standard storage initialization because migration was performed"
                );
            }
        }
    }

    if req.ha_type == HaType::Generic {
        if let Some(ref migration_opts) = req.migration {
            if migration_opts.migrate_data {
                state.send_progress(
                    &operation_id,
                    "create_ha_profile",
                    Some(&req.name),
                    40,
                    "Stopping services on all nodes...",
                    false,
                    None,
                );

                let remote_sys = RemoteSystemdController::new(state.ssh_manager.clone());
                let credential = crate::core::SshCredential::Password("ignored".to_string());

                for service in &req.services {
                    for node in &all_nodes {
                        tracing::info!("Stopping service {} on {}", service, node.hostname);
                        if node.is_local {
                            if let Ok(sys) = SystemdController::new().await {
                                let _ = sys.stop(service).await;
                            }
                        } else if let Err(e) = remote_sys
                            .stop(
                                &node.ip,
                                node.ssh_port,
                                &node.ssh_user,
                                &credential,
                                service,
                            )
                            .await
                        {
                            tracing::warn!(
                                "Failed to stop service {} on {}: {}",
                                service,
                                node.hostname,
                                e
                            );
                        }
                    }
                }

                state.send_progress(
                    &operation_id,
                    "create_ha_profile",
                    Some(&req.name),
                    42,
                    "Starting data migration...",
                    false,
                    None,
                );

                let migration_config = MigrationConfig {
                    resource_name: req.resource_name.clone(),
                    source_path: migration_opts
                        .source_path
                        .clone()
                        .unwrap_or_else(|| req.mount_point.clone()),
                    mount_point: req.mount_point.clone(),
                    fs_type: req.fs_type.clone(),
                    format_device: migration_opts.format_device,
                    services_to_stop: req.services.clone(),
                    preserve_permissions: migration_opts.preserve_permissions,
                    keep_primary: true,
                };

                match DataMigration::migrate(migration_config, None).await {
                    Ok(result) => {
                        state.send_progress(
                            &operation_id,
                            "create_ha_profile",
                            Some(&req.name),
                            45,
                            &format!(
                                "Migration done ({} bytes). Waiting for DRBD sync...",
                                result.bytes_transferred
                            ),
                            false,
                            None,
                        );

                        let remote_nodes: Vec<_> =
                            all_nodes.iter().filter(|n| !n.is_local).collect();
                        for node in &remote_nodes {
                            let credential =
                                crate::core::SshCredential::Password("ignored".to_string());
                            let cmd = DrbdCmd::secondary_cmd(&req.resource_name)?;
                            tracing::info!(
                                "Ensuring remote node {} is Secondary for {}",
                                node.hostname,
                                req.resource_name
                            );
                            if let Err(e) = state
                                .ssh_manager
                                .execute(&node.ip, node.ssh_port, &node.ssh_user, &credential, &cmd)
                                .await
                            {
                                tracing::warn!(
                                    "Failed to set remote node {} to Secondary: {}",
                                    node.hostname,
                                    e
                                );
                            }
                        }

                        let mut sync_attempts = 0;
                        let max_attempts = 120;
                        loop {
                            match drbd_utils::get_drbd_sync_status(&req.resource_name).await {
                                Ok(status) => {
                                    if status.local_role == "Primary"
                                        && status.local_disk_state == "UpToDate"
                                    {
                                        state.send_progress(
                                            &operation_id,
                                            "create_ha_profile",
                                            Some(&req.name),
                                            48,
                                            "Local resource ready. Proceeding with configuration (Background sync active)...",
                                            false,
                                            None,
                                        );

                                        let bg_state = state.clone();
                                        let resource_name = req.resource_name.clone();
                                        let profile_name = req.name.clone();
                                        let bg_op_id = operation_id.clone();

                                        tokio::spawn(async move {
                                            let mut attempts = 0;
                                            while attempts < 360 {
                                                tokio::time::sleep(
                                                    tokio::time::Duration::from_secs(5),
                                                )
                                                .await;
                                                match drbd_utils::get_drbd_sync_status(
                                                    &resource_name,
                                                )
                                                .await
                                                {
                                                    Ok(s) if s.is_fully_synced => {
                                                        bg_state.send_progress(
                                                            &bg_op_id,
                                                            "create_ha_profile_sync",
                                                            Some(&profile_name),
                                                            100,
                                                            "DRBD resource fully synced (Background task)",
                                                            true,
                                                            Some(true),
                                                        );
                                                        break;
                                                    }
                                                    Ok(s) => {
                                                        if let Some(p) = s.sync_progress_percent {
                                                            tracing::debug!(
                                                                "Background sync for {}: {:.2}%",
                                                                resource_name,
                                                                p
                                                            );
                                                        }
                                                    }
                                                    Err(e) => {
                                                        tracing::warn!(
                                                            "Background sync check failed: {}",
                                                            e
                                                        );
                                                    }
                                                }
                                                attempts += 1;
                                            }
                                        });

                                        break;
                                    }

                                    if status.is_fully_synced {
                                        state.send_progress(
                                            &operation_id,
                                            "create_ha_profile",
                                            Some(&req.name),
                                            48,
                                            "DRBD resource fully synced",
                                            false,
                                            None,
                                        );
                                        break;
                                    } else {
                                        let msg =
                                            if let Some(percent) = status.sync_progress_percent {
                                                format!(
                                                    "Waiting for local DRBD ready: {:.2}% synced",
                                                    percent
                                                )
                                            } else {
                                                format!(
                                                    "Waiting for local DRBD ready (Status: {}/{})",
                                                    status.local_role, status.local_disk_state
                                                )
                                            };

                                        state.send_progress(
                                            &operation_id,
                                            "create_ha_profile",
                                            Some(&req.name),
                                            45,
                                            &msg,
                                            false,
                                            None,
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to check DRBD sync status: {}", e);
                                }
                            }

                            sync_attempts += 1;
                            if sync_attempts >= max_attempts {
                                return Err(AppError::Timeout(
                                    format!("DRBD sync timeout for resource {} after {} attempts ({} minutes)",
                                           req.resource_name, max_attempts, max_attempts * 5 / 60)
                                ));
                            }

                            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        }

                        messages.push("Data migration completed successfully".to_string());
                    }
                    Err(e) => {
                        state.send_progress(
                            &operation_id,
                            "create_ha_profile",
                            Some(&req.name),
                            40,
                            &format!("Migration failed: {}", e),
                            true,
                            None,
                        );
                        return Err(AppError::Migration(format!("Data migration failed: {}", e)));
                    }
                }
            }
        }
    }

    let _start_services = match req.ha_type {
        HaType::Generic => {
            state.send_progress(
                &operation_id,
                "create_ha_profile",
                Some(&req.name),
                30,
                "Generating service overrides...",
                false,
                None,
            );
            if let Some(mount_unit) = &generated_units.mount_unit {
                let override_infos = ServiceOverrideGenerator::generate_for_services(
                    &req.services,
                    mount_unit,
                    &req.name,
                )
                .await?;

                for info in override_infos {
                    generated_units.service_overrides.push(ServiceOverride {
                        service_name: info.service_name.clone(),
                        override_dir: info.override_dir,
                        override_path: info.override_path,
                    });
                }
                messages.push(format!(
                    "Generated {} service override(s)",
                    req.services.len()
                ));
            }

            req.services.clone()
        }
    };

    state.send_progress(
        &operation_id,
        "create_ha_profile",
        Some(&req.name),
        55,
        "Generating promoter configuration...",
        false,
        None,
    );

    let profile = HaProfile {
        id: profile_id.clone(),
        name: req.name.clone(),
        ha_type: req.ha_type.clone(),
        resource_name: req.resource_name.clone(),
        mount_point: req.mount_point.clone(),
        fs_type: req.fs_type.clone(),
        mount_strategy: req.mount_strategy.clone(),
        vip: req.vip.clone(),
        ocf_agents: req.ocf_agents.clone(),
        promoter: PromoterSettings {
            services: req.services.clone(),
            stop_on_demote: req.stop_on_demote,
            on_demote_failure: req.on_demote_failure.clone(),
            preferred_nodes: req.preferred_nodes.clone(),
            preferred_nodes_policy: req.preferred_nodes_policy.clone(),
            sleep_before_promote_factor: req.sleep_before_promote_factor,
            dependencies_as: req.dependencies_as.clone(),
            target_as: req.target_as.clone(),
            on_quorum_loss: req.on_quorum_loss.clone(),
        },
        status: HaProfileStatus::Unknown,
        active_node: None,
        generated_units: generated_units.clone(),
        is_builtin_plugin: false,
    };

    let reactor_config_gen = ReactorConfigGenerator::new()?;
    let mut profile_for_gen = profile.clone();
    profile_for_gen.promoter.services = _start_services.clone();

    let promoter_config = drbd_reactor_utils::PromoterConfig {
        resource: profile_for_gen.resource_name.clone(),
        mount_unit: profile_for_gen.generated_units.mount_unit.clone(),
        start: _start_services.clone(),
        stop_services_on_exit: profile_for_gen.promoter.stop_on_demote,
        on_drbd_demote_failure: profile_for_gen.promoter.on_demote_failure.clone(),
        vip: profile_for_gen.vip.as_ref().map(|v| drbd_reactor_utils::VipConfig {
            address: v.address.clone(),
            netmask: v.netmask,
        }),
        ocf_agents: profile_for_gen.ocf_agents.iter().map(|a| drbd_reactor_utils::OcfAgentConfig {
            name: a.name.clone(),
            instance_name: a.instance_name.clone(),
            params: a.params.clone(),
        }).collect(),
        mount_strategy: Some(format!("{:?}", profile_for_gen.mount_strategy).to_lowercase()),
        mount_point: Some(profile_for_gen.mount_point.clone()),
        fs_type: Some(profile_for_gen.fs_type.clone()),
        dependencies_as: profile_for_gen.promoter.dependencies_as.clone(),
        target_as: profile_for_gen.promoter.target_as.clone(),
        on_quorum_loss: profile_for_gen.promoter.on_quorum_loss.clone(),
        preferred_nodes: profile_for_gen.promoter.preferred_nodes.clone(),
        preferred_nodes_policy: profile_for_gen.promoter.preferred_nodes_policy.clone(),
        sleep_before_promote_factor: profile_for_gen.promoter.sleep_before_promote_factor,
    };
    let config_content = reactor_config_gen.generate_promoter(&promoter_config)?;

    // Determine config path based on start_disabled flag
    let normal_config_path = ReactorConfigPaths::promoter_path(&req.name);
    let config_path = if req.start_disabled {
        format!("{}.disabled", normal_config_path)
    } else {
        normal_config_path.clone()
    };

    let config_dir = std::path::Path::new(&config_path).parent().unwrap();
    if !config_dir.exists() {
        tokio::fs::create_dir_all(config_dir)
            .await
            .map_err(|e| AppError::Config(format!("Failed to create config dir: {}", e)))?;
    }

    tokio::fs::write(&config_path, &config_content)
        .await
        .map_err(|e| AppError::Config(format!("Failed to write promoter config: {}", e)))?;
    messages.push(if req.start_disabled {
        "Generated promoter configuration (disabled)".to_string()
    } else {
        "Generated promoter configuration".to_string()
    });

    state.send_progress(
        &operation_id,
        "create_ha_profile",
        Some(&req.name),
        60,
        "Configuring managed services...",
        false,
        None,
    );

    let services_to_disable = req.services.clone();

    let mut disabled_services = Vec::new();
    if req.auto_disable_services && !services_to_disable.is_empty() {
        let systemd = SystemdController::new().await?;
        for service in &services_to_disable {
            if systemd.is_enabled(service).await.unwrap_or(false) {
                if let Ok(()) = systemd.disable_and_stop(service).await {
                    disabled_services.push(service.clone());
                }
            }
        }

        let remote_sys = RemoteSystemdController::new(state.ssh_manager.clone());
        let remote_nodes: Vec<_> = all_nodes.iter().filter(|n| !n.is_local).collect();
        for node in &remote_nodes {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            for service in &services_to_disable {
                let _ = remote_sys
                    .disable_and_stop(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        service,
                    )
                    .await;
            }
        }

        if !disabled_services.is_empty() {
            messages.push(format!(
                "Disabled {} service(s) on all nodes",
                disabled_services.len()
            ));
        }
    }

    if let Ok(sys) = SystemdController::new().await {
        let _ = sys.daemon_reload().await;
    }

    state.send_progress(
        &operation_id,
        "create_ha_profile",
        Some(&req.name),
        75,
        "Syncing configuration to cluster nodes...",
        false,
        None,
    );

    let mut synced_nodes = Vec::new();
    {
        let mount_unit_content = if let Some(ref path) = generated_units.mount_unit_path {
            tokio::fs::read_to_string(path).await.ok()
        } else {
            None
        };

        let mut service_override_contents = Vec::new();
        for so in &generated_units.service_overrides {
            if let Ok(content) = tokio::fs::read_to_string(&so.override_path).await {
                service_override_contents.push((so.override_path.clone(), content));
            }
        }

        let remote_nodes: Vec<_> = all_nodes.iter().filter(|n| !n.is_local).collect();
        for (path, content) in extra_sync_files {
            for node in &remote_nodes {
                let credential = crate::core::SshCredential::Password("ignored".to_string());
                let _ = state
                    .ssh_manager
                    .write_file(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &path,
                        &content,
                    )
                    .await;
            }
        }

        let drbd_config_path = if let Some(ref path) = generated_units.drbd_config_path {
            tokio::fs::read_to_string(path)
                .await
                .ok()
                .map(|content| (path.clone(), content))
        } else {
            None
        };

        let sync_config = HaSyncConfig {
            drbd_resource_config: drbd_config_path,
            mount_unit: generated_units
                .mount_unit_path
                .clone()
                .zip(mount_unit_content),
            service_overrides: service_override_contents,
            promoter_config: (config_path.clone(), config_content.clone()),
        };

        let cluster_sync = ClusterSync::new(
            state.ssh_manager.clone(),
            state.node_store.clone(),
            state.credentials.clone(),
        );

        if let Ok(nodes) = cluster_sync.sync_ha_config(&sync_config).await {
            if !nodes.is_empty() {
                messages.push(format!("Synced to {} node(s)", nodes.len()));
                synced_nodes = nodes;
            }
        }
    }

    state.send_progress(
        &operation_id,
        "create_ha_profile",
        Some(&req.name),
        95,
        if req.start_disabled {
            "Skipping drbd-reactor reload (profile is disabled)..."
        } else {
            "Reloading drbd-reactor on all nodes to apply configuration..."
        },
        false,
        None,
    );

    // Only reload drbd-reactor if not starting in disabled mode
    if !req.start_disabled {
        let all_nodes = state.node_store.get_all()?;
        let mut successful_reloads = 0;
        let mut total_nodes = 0;

        info!("Reloading drbd-reactor service on all nodes to apply new HA configuration");

        for node in &all_nodes {
            total_nodes += 1;
            let hostname = &node.hostname;

            if node.is_local {
                let sys = SystemdController::new().await;
                if let Ok(sys) = sys {
                    match sys.reload("drbd-reactor.service").await {
                        Ok(()) => {
                            info!("Successfully reloaded drbd-reactor on local node");
                            successful_reloads += 1;
                        }
                        Err(e) => {
                            tracing::warn!("Failed to reload drbd-reactor on local node: {}", e);
                        }
                    }
                } else {
                    tracing::warn!(
                        "Failed to initialize systemd controller for local drbd-reactor reload"
                    );
                }
            } else {
                let cred = crate::core::SshCredential::Password("ignored".to_string());
                let reload_cmd = format!("sudo {}", SystemdCmd::reload_service_cmd("drbd-reactor.service"));

                match state
                    .ssh_manager
                    .execute(&node.ip, node.ssh_port, &node.ssh_user, &cred, &reload_cmd)
                    .await
                {
                    Ok(output) => {
                        if output.success() {
                            info!(
                                "Successfully reloaded drbd-reactor on remote node: {}",
                                hostname
                            );
                            successful_reloads += 1;
                        } else {
                            tracing::warn!(
                                "Failed to reload drbd-reactor on remote node {}: {}",
                                hostname,
                                output.stderr
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to connect to remote node {} for drbd-reactor reload: {}",
                            hostname,
                            e
                        );
                    }
                }
            }
        }

        if successful_reloads == total_nodes {
            info!(
                "Successfully reloaded drbd-reactor on all {}/{} nodes",
                successful_reloads, total_nodes
            );
            messages.push(format!(
                "HA profile created and drbd-reactor reloaded on all {} nodes",
                successful_reloads
            ));

            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        } else {
            tracing::warn!(
                "drbd-reactor reload succeeded on only {}/{} nodes",
                successful_reloads,
                total_nodes
            );
            messages.push(format!(
                "HA profile created but drbd-reactor reload failed on {}/{} nodes",
                total_nodes - successful_reloads,
                total_nodes
            ));
        }
    } else {
        // Profile was created in disabled mode
        messages.push("HA profile created in disabled mode. Activate it when ready.".to_string());
    }

    state.send_progress(
        &operation_id,
        "create_ha_profile",
        Some(&req.name),
        100,
        "HA profile created successfully",
        true,
        Some(true),
    );
    state.send_notification(
        NotificationLevel::Success,
        "HA Profile Created",
        &format!("HA profile '{}' created successfully{}", req.name, if req.start_disabled { " (disabled)" } else { "" }),
    );

    // Read DRBD config content for preview
    // If we created a new DRBD config, read it from the generated path
    // Otherwise, try to read the existing DRBD resource config
    let drbd_config_content = if let Some(ref path) = generated_units.drbd_config_path {
        tokio::fs::read_to_string(path).await.ok()
    } else {
        // Try to read existing DRBD resource config
        let existing_config_path = DrbdConfigPaths::drbd_resource_path(&req.resource_name);
        tokio::fs::read_to_string(&existing_config_path).await.ok()
    };

    Ok((
        StatusCode::CREATED,
        Json(HaProfileCreateResponse {
            profile,
            config_path,
            message: messages.join(". ") + ".",
            disabled_services,
            generated_units: Some(generated_units),
            migration_result,
            synced_nodes,
            promoter_config_content: Some(config_content),
            drbd_config_content,
        }),
    ))
}

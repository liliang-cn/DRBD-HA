use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use drbd_migration::{DataMigration, MigrationConfig};
use drbd_reactor_utils::DrbdReactorClient;
use drbd_utils::parse_drbdadm_status;
use std::sync::Arc;
use tracing::info;

use crate::core::{
    cluster_sync::{ClusterSync, HaSyncConfig},
    config_gen::{ConfigGenerator, ConfigPaths, NodeConfig, ResourceConfig},
    drbd_cmd::DrbdCmd,
    mount_unit::MountUnitGenerator,
    run_shell_command,
    service_override::ServiceOverrideGenerator,
    systemd_ctrl::{RemoteSystemdController, SystemdController},
    validator, IscsiGenerator, LvmProvider, NfsGenerator, NvmeOfGenerator, ServiceInitFactory,
    StorageProvider, ZfsProvider,
};
use crate::error::{AppError, AppResult};
use crate::models::{
    CreateHaProfileRequest, GeneratedUnits, HaProfile, HaProfileStatus, HaType, Node,
    PromoterSettings, ServiceOverride,
};
use crate::state::{AppState, NotificationLevel};

use super::types::*;
use super::utils::*;

/// GET /api/v1/ha/profiles
pub async fn list_profiles(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<HaProfileListResponse>> {
    let mut profiles = state.db.get_all_ha_profiles()?;

    for profile in &mut profiles {
        let config_path = ConfigPaths::promoter_path(&profile.name);
        if let Ok(content) = tokio::fs::read_to_string(&config_path).await {
            if profile.vip.is_none() {
                if let Some(vip_info) = parse_vip_from_config(&content) {
                    profile.vip = Some(vip_info);
                }
            }
            if profile.mount_point.is_empty() {
                if let Some(mount_pt) = parse_mount_point_from_config(&content) {
                    profile.mount_point = mount_pt;
                }
            }
        }

        if let Ok((statuses, _)) = DrbdReactorClient::status(Some(&profile.name)).await {
            if let Some(status) = statuses.first() {
                if status.is_active {
                    profile.status = HaProfileStatus::Active;
                    profile.active_node = status.active_node.clone();
                } else {
                    profile.status = HaProfileStatus::Standby;
                    profile.active_node = None;
                }
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
    let mut profile = state
        .db
        .get_ha_profile(&id_or_name)?
        .or(state.db.get_ha_profile_by_name(&id_or_name)?)
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    // Enrich profile with data from config file if missing
    let config_path = ConfigPaths::promoter_path(&profile.name);
    if let Ok(content) = tokio::fs::read_to_string(&config_path).await {
        if profile.vip.is_none() {
            if let Some(vip_info) = parse_vip_from_config(&content) {
                profile.vip = Some(vip_info);
            }
        }
        if profile.mount_point.is_empty() {
            if let Some(mount_pt) = parse_mount_point_from_config(&content) {
                profile.mount_point = mount_pt;
            }
        }
    }

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

    // Update the profile status in the return object (but not DB to avoid thrashing)
    // Actually, HaProfileDetailResponse has a 'status' field that overrides profile.status
    // But we also embed profile. Let's make sure the embedded profile has the correct status if we want consistent view.
    // However, the structs fields override embedded ones in Serde if they have same name?
    // No, flattening puts fields at same level. Duplicate fields error?
    // `HaProfile` has `status`. `HaProfileDetailResponse` has `status`.
    // Serde will complain if we flatten and have same field name?
    // "If a struct has a field with the same name as a field in the flattened struct, the field in the outer struct takes precedence."
    // Wait, let's verify serde behavior. Usually it errors on duplicate keys during deserialization, but for serialization it might write both? Or one overwrites?
    // Actually, `HaProfile` has `status`. `HaProfileDetailResponse` has `status`.
    // It's better to NOT have `status` in `HaProfileDetailResponse` explicitly if we want to use the one from `HaProfile`, OR update `profile.status` before returning.
    // I'll update `profile.status` and remove `status` from `HaProfileDetailResponse`.

    let mut profile_out = profile.clone();
    profile_out.status = status.clone();

    Ok(Json(HaProfileDetailResponse {
        profile: profile_out,
        status, // Keeping it for backward compatibility with status endpoint
        active_node,
        mount_point,
        drbd,
        service_statuses,
        vip_active,
        config,
        reactor_status_raw,
    }))
}

/// POST /api/v1/ha/profiles
pub async fn create_profile(
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<CreateHaProfileRequest>,
) -> AppResult<(StatusCode, Json<HaProfileCreateResponse>)> {
    let operation_id = uuid::Uuid::new_v4().to_string();
    let mut generated_units = GeneratedUnits::default(); // Initialize at function start
                                                         // Migration status is calculated below based on migration options
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

    if matches!(req.ha_type, HaType::Generic | HaType::Nfs) {
        validator::validate_mount_point(&req.mount_point)?;
    }

    validator::validate_fs_type(&req.fs_type)?;

    if req.ha_type == HaType::Generic {
        for service in &req.services {
            validator::validate_service_name(service)?;
        }
    } else {
        // For non-Generic types, we override the services list to ensure it contains
        // only the expected systemd services (and valid names), ignoring any UI garbage.
        // This prevents "Invalid service name" errors during activation if the UI sent
        // incomplete names (e.g. "nfs-server" without .service) or unrelated services.
        match req.ha_type {
            HaType::Nfs => req.services = vec!["nfs-server.service".to_string()],
            HaType::Iscsi => req.services = vec!["target.service".to_string()],
            HaType::NvmeOf => req.services = vec![], // NVMe-oF uses kernel target
            _ => {}
        }
    }

    if let Some(vip) = &mut req.vip {
        // If interface is empty, "auto", or default "eth0", try to detect actual interface
        if vip.interface.is_empty() || vip.interface == "auto" || vip.interface == "eth0" {
            if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
                let mut interfaces = Vec::new();
                for entry in entries.flatten() {
                    if let Ok(name) = entry.file_name().into_string() {
                        // Skip loopback and common virtual interfaces
                        if name != "lo"
                            && !name.starts_with("docker")
                            && !name.starts_with("veth")
                            && !name.starts_with("br-")
                        {
                            interfaces.push(name);
                        }
                    }
                }
                // Sort to have consistent selection (e.g. eth0 before eth1)
                interfaces.sort();
                if let Some(first) = interfaces.first() {
                    tracing::info!(
                        "Auto-detected VIP interface: {} (was {})",
                        first,
                        vip.interface
                    );
                    vip.interface = first.clone();
                }
            }
        }
        validator::validate_ip_address(&vip.address)?;
        validator::validate_netmask(vip.netmask)?;
    }

    if state.db.ha_profile_name_exists(&req.name)? {
        return Err(AppError::AlreadyExists(format!(
            "HA profile with name {} already exists",
            req.name
        )));
    }

    let profile_id = uuid::Uuid::new_v4().to_string();
    let all_nodes = state.db.get_all_nodes()?;

    // Resolve DRBD port and minor early to prevent conflicts and for param injection
    let drbd_port = if let Some(p) = req.drbd_port {
        p
    } else {
        super::utils::find_next_free_drbd_port().await?
    };

    let drbd_minor = if let Some(m) = req.drbd_minor {
        m
    } else {
        super::utils::find_next_free_drbd_minor().await?
    };

    // Auto-configure manual Filesystem OCF agent if present
    // This allows users to add 'Filesystem' via the manual agent list and have it auto-configured
    // with the correct DRBD device path, avoiding the need to manually type it.
    let mut manual_filesystem_agent_present = false;
    for agent in &mut req.ocf_agents {
        if agent.name == "ocf:heartbeat:Filesystem" {
            manual_filesystem_agent_present = true;

            // Auto-fill 'device' if missing or empty
            if !agent.params.contains_key("device")
                || agent
                    .params
                    .get("device")
                    .map(|s| s.is_empty())
                    .unwrap_or(true)
            {
                // Try to detect device path, fallback to standard convention
                let drbd_device = format!("/dev/drbd{}", drbd_minor);
                agent.params.insert("device".to_string(), drbd_device);
            }

            // Auto-fill 'directory' if missing or empty
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

            // Auto-fill 'fstype' if missing or empty
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

    // Validate OCF agents after potential injection
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
        tracing::info!("Creating LVM volume for profile {}", req.name);

        let storage_pool = state
            .db
            .get_storage_pool(pool_id)?
            .ok_or_else(|| AppError::NotFound(format!("Storage pool '{}' not found", pool_id)))?;

        let mut node_lvm_paths: Vec<(Node, String)> = Vec::new();

        for node in &all_nodes {
            let current_vg_name = storage_pool.name.clone();
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

            if node.is_local {
                let existing_volumes = state.db.get_all_volumes_in_pool(&storage_pool.id)?;
                let volume_exists = existing_volumes.iter().any(|v| v.name == lv_name);

                if !volume_exists {
                    let new_volume = crate::models::Volume {
                        id: uuid::Uuid::new_v4().to_string(),
                        pool_id: storage_pool.id.clone(),
                        name: lv_name.clone(),
                        size_gb: *volume_size_gb,
                        device_path: device_path.clone(),
                        drbd_res: Some(req.resource_name.clone()),
                    };
                    state.db.insert_volume(&new_volume)?;
                }
            }

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

        let config_gen = ConfigGenerator::new()?;
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
        let config_path = ConfigPaths::drbd_resource_path(&req.resource_name);

        tokio::fs::write(&config_path, &config_content)
            .await
            .map_err(|e| AppError::Config(format!("Failed to write DRBD config: {}", e)))?;

        // Store the DRBD config path for cluster synchronization
        generated_units.drbd_config_path = Some(config_path.clone());

        let remote_nodes: Vec<_> = all_nodes.iter().filter(|n| !n.is_local).collect();
        for node in &remote_nodes {
            let credential = crate::core::SshCredential::Password("ignored".to_string());

            // Clone state per iteration to avoid moving the Arc across loop iterations
            let state_for_write = state.clone();

            // 同步DRBD配置文件到远程节点
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

            // 使用 drbd-utils 验证远程节点上的DRBD配置状态
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

                        // Log detailed verification information
                        tracing::debug!("DRBD verification details for node {}: connected_peers={}, consistent={}",
                            node.hostname, result.details.connected_peers, result.details.is_consistent);
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

        let create_md_cmd = format!("drbdadm create-md --force {}", req.resource_name);
        let up_cmd = format!("drbdadm up {}", req.resource_name);

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
    // Handle ZFS volume creation
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
        tracing::info!("Creating ZFS volume for profile {}", req.name);

        let storage_pool = state
            .db
            .get_storage_pool(pool_id)?
            .ok_or_else(|| AppError::NotFound(format!("Storage pool '{}' not found", pool_id)))?;

        let mut node_zfs_paths: Vec<(Node, String)> = Vec::new();

        for node in &all_nodes {
            let current_pool_name = storage_pool.name.clone();
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

            if node.is_local {
                // Check if we need to add ZFS volume to database
                // The current Volume model might need to be extended to support ZFS
                // For now, we'll just log that the volume was created
                tracing::info!(
                    "Created ZFS volume '{}' on node {} at '{}'",
                    zvol_name,
                    node.hostname,
                    device_path
                );
            }

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

        let config_gen = ConfigGenerator::new()?;
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
        let config_path = ConfigPaths::drbd_resource_path(&req.resource_name);

        let config_dir = std::path::Path::new(&config_path).parent().unwrap();
        if !config_dir.exists() {
            tokio::fs::create_dir_all(config_dir)
                .await
                .map_err(|e| AppError::Config(format!("Failed to create config dir: {}", e)))?;
        }

        tokio::fs::write(&config_path, &config_content)
            .await
            .map_err(|e| AppError::Config(format!("Failed to write DRBD config: {}", e)))?;

        // Store the DRBD config path for cluster synchronization
        generated_units.drbd_config_path = Some(config_path.clone());

        let remote_nodes: Vec<_> = all_nodes.iter().filter(|n| !n.is_local).collect();
        for node in &remote_nodes {
            let credential = crate::core::SshCredential::Password("ignored".to_string());

            // Clone state per iteration to avoid moving the Arc across loop iterations
            let state_for_write = state.clone();

            // 同步DRBD配置文件到远程节点
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

            // 使用 drbd-utils 验证远程节点上的DRBD配置状态
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

                        // Log detailed verification information
                        tracing::debug!("DRBD verification details for node {}: connected_peers={}, consistent={}",
                            node.hostname, result.details.connected_peers, result.details.is_consistent);
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

        let create_md_cmd = format!("drbdadm create-md --force {}", req.resource_name);
        let up_cmd = format!("drbdadm up {}", req.resource_name);

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

    if matches!(req.ha_type, HaType::Generic | HaType::Nfs) {
        // Only generate systemd mount unit if we are NOT using a manual Filesystem OCF agent
        // and NOT using OCF mount strategy (which is handled by ConfigGenerator automatically)
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
            // We still need to record the expected DRBD device for other operations
            generated_units.drbd_device = Some(format!("/dev/drbd{}", drbd_minor));
        }
    }

    // Store OCF exportfs config for NFS to use in the second pass
    let mut ocf_exportfs_cache: Option<String> = None;

    // Check if migration will be performed to skip storage initialization
    let migration_performed = if matches!(req.ha_type, HaType::Generic | HaType::Nfs) {
        if let Some(ref migration_opts) = req.migration {
            migration_opts.migrate_data
        } else {
            false
        }
    } else {
        false
    };

    // First pass: handle storage initialization and service setup
    // Service overrides will be generated in the second pass after migration logic
    match req.ha_type {
        HaType::Generic => {
            // Storage initialization for Generic services
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
                    &format!("drbdadm primary {}", req.resource_name),
                    "Promote for setup",
                )
                .await?;
                let mkfs_cmd = format!("mkfs.{} /dev/drbd{}", req.fs_type, drbd_minor);
                run_shell_command(&mkfs_cmd, "Create filesystem").await?;

                run_shell_command(
                    &format!("mkdir -p {}", req.mount_point),
                    "Create mount point",
                )
                .await?;
                run_shell_command(
                    &format!("mount /dev/drbd{} {}", drbd_minor, req.mount_point),
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
                                    &format!("drbdadm secondary {}", req.resource_name),
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
                    &format!("drbdadm secondary {}", req.resource_name),
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
        HaType::Nfs => {
            state.send_progress(
                &operation_id,
                "create_ha_profile",
                Some(&req.name),
                30,
                "Configuring NFS...",
                false,
                None,
            );
            let nfs_config = req
                .nfs
                .as_ref()
                .ok_or_else(|| AppError::Validation("NFS configuration missing".to_string()))?;

            if (req.lvm_pool_id.is_some() || req.zfs_pool_id.is_some()) && !migration_performed {
                state.send_progress(
                    &operation_id,
                    "create_ha_profile",
                    Some(&req.name),
                    35,
                    "Initializing NFS state storage...",
                    false,
                    None,
                );

                run_shell_command(
                    &format!("drbdadm primary {}", req.resource_name),
                    "Promote for NFS setup",
                )
                .await?;

                let mkfs_cmd = format!("mkfs.{} /dev/drbd{}", req.fs_type, drbd_minor);
                run_shell_command(&mkfs_cmd, "Create filesystem for NFS state").await?;

                run_shell_command(
                    &format!("mkdir -p {}", req.mount_point),
                    "Create mount point",
                )
                .await?;
                run_shell_command(
                    &format!("mount /dev/drbd{} {}", drbd_minor, req.mount_point),
                    "Mount for NFS setup",
                )
                .await?;

                if let Err(e) = NfsGenerator::setup_nfs_state(&req.mount_point).await {
                    let _ =
                        run_shell_command(&format!("umount {}", req.mount_point), "Cleanup umount")
                            .await;
                    let _ = run_shell_command(
                        &format!("drbdadm secondary {}", req.resource_name),
                        "Cleanup secondary",
                    )
                    .await;
                    return Err(e);
                }

                run_shell_command(
                    &format!("umount {}", req.mount_point),
                    "Unmount after NFS setup",
                )
                .await?;
                run_shell_command(
                    &format!("drbdadm secondary {}", req.resource_name),
                    "Demote after NFS setup",
                )
                .await?;

                let remote_nodes: Vec<_> = all_nodes.iter().filter(|n| !n.is_local).collect();
                for node in &remote_nodes {
                    let credential = crate::core::SshCredential::Password("ignored".to_string());
                    let remote_cmd = format!(
                        "systemctl stop nfs-server && \
                            [ -d /var/lib/nfs ] && [ ! -L /var/lib/nfs ] && mv /var/lib/nfs /var/lib/nfs.bak || true && \
                            rm -rf /var/lib/nfs && \
                            ln -s {}/.nfs_state /var/lib/nfs",
                        req.mount_point
                    );
                    state
                        .ssh_manager
                        .execute(
                            &node.ip,
                            node.ssh_port,
                            &node.ssh_user,
                            &credential,
                            &remote_cmd,
                        )
                        .await?;
                }
            } else if (req.lvm_pool_id.is_some() || req.zfs_pool_id.is_some())
                && migration_performed
            {
                tracing::info!(
                    "Skipping NFS storage initialization because migration was performed"
                );
            }

            state.send_progress(
                &operation_id,
                "create_ha_profile",
                Some(&req.name),
                40,
                "Configuring OCF Exportfs...",
                false,
                None,
            );

            // Calculate FSID and generate OCF resource string
            let fsid = NfsGenerator::generate_fsid(&req.resource_name);
            ocf_exportfs_cache = Some(NfsGenerator::generate_ocf_exportfs(
                &req.resource_name,
                &req.mount_point,
                nfs_config,
                fsid,
            ));
        }
        HaType::Iscsi => {
            state.send_progress(
                &operation_id,
                "create_ha_profile",
                Some(&req.name),
                30,
                "Configuring iSCSI Target...",
                false,
                None,
            );
            if let (Some(iscsi_config), Some(vip)) = (&req.iscsi, &req.vip) {
                let drbd_dev = format!("/dev/drbd{}", req.drbd_minor.unwrap_or(0));

                let real_setup_cmds = IscsiGenerator::generate_setup_commands(
                    &req.resource_name,
                    &drbd_dev,
                    iscsi_config,
                    &vip.address,
                );

                let creds = state.credentials.read().await;
                for node in &all_nodes {
                    let cmd_str = real_setup_cmds.join(" && ");
                    if node.is_local {
                        run_shell_command(&cmd_str, "Setup iSCSI Target locally").await?;
                    } else {
                        let credential =
                            crate::core::SshCredential::Password("ignored".to_string());
                        state
                            .ssh_manager
                            .execute(
                                &node.ip,
                                node.ssh_port,
                                &node.ssh_user,
                                &credential,
                                &cmd_str,
                            )
                            .await?;
                    }
                }
                drop(creds);
                messages.push("Configured iSCSI Target on all nodes".to_string());
            } else {
                return Err(AppError::Validation(
                    "iSCSI configuration or VIP missing".to_string(),
                ));
            }
        }
        HaType::NvmeOf => {
            state.send_progress(
                &operation_id,
                "create_ha_profile",
                Some(&req.name),
                30,
                "Configuring NVMe-oF Target...",
                false,
                None,
            );
            if let (Some(nvmeof_config), Some(vip)) = (&req.nvmeof, &req.vip) {
                let drbd_dev = format!("/dev/drbd{}", req.drbd_minor.unwrap_or(0));
                let setup_cmds = NvmeOfGenerator::generate_setup_commands(
                    &req.resource_name,
                    &drbd_dev,
                    nvmeof_config,
                    &vip.address,
                );

                let creds = state.credentials.read().await;
                for node in &all_nodes {
                    let cmd_str = setup_cmds.join(" && ");
                    if node.is_local {
                        run_shell_command(&cmd_str, "Setup NVMe-oF Target locally").await?;
                    } else {
                        let credential =
                            crate::core::SshCredential::Password("ignored".to_string());
                        state
                            .ssh_manager
                            .execute(
                                &node.ip,
                                node.ssh_port,
                                &node.ssh_user,
                                &credential,
                                &cmd_str,
                            )
                            .await?;
                    }
                }
                drop(creds);
                messages.push("Configured NVMe-oF Target on all nodes".to_string());
            } else {
                return Err(AppError::Validation(
                    "NVMe-oF configuration or VIP missing".to_string(),
                ));
            }
        }
    }

    if matches!(req.ha_type, HaType::Generic | HaType::Nfs) {
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

                // Stop services on all nodes (local and remote) to ensure data consistency
                let remote_sys = RemoteSystemdController::new(state.ssh_manager.clone());
                let credential = crate::core::SshCredential::Password("ignored".to_string());

                for service in &req.services {
                    for node in &all_nodes {
                        tracing::info!("Stopping service {} on {}", service, node.hostname);
                        if node.is_local {
                            if let Ok(sys) = SystemdController::new().await {
                                let _ = sys.stop(service).await;
                            }
                        } else {
                            if let Err(e) = remote_sys
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

                        // Ensure remote nodes are Secondary
                        let remote_nodes: Vec<_> =
                            all_nodes.iter().filter(|n| !n.is_local).collect();
                        for node in &remote_nodes {
                            let credential =
                                crate::core::SshCredential::Password("ignored".to_string());
                            let cmd = format!("drbdadm secondary {}", req.resource_name);
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

                        // 同步等待DRBD完全同步
                        let mut sync_attempts = 0;
                        let max_attempts = 120; // 最多等待10分钟
                        loop {
                            match drbd_utils::get_drbd_sync_status(&req.resource_name).await {
                                Ok(status) => {
                                    // Optimization: If local node is Primary and UpToDate, we can proceed
                                    // while the sync continues in the background.
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

                                        // Spawn a background task to monitor full sync completion
                                        let bg_state = state.clone();
                                        let resource_name = req.resource_name.clone();
                                        let profile_name = req.name.clone();
                                        let bg_op_id = operation_id.clone();

                                        tokio::spawn(async move {
                                            let mut attempts = 0;
                                            // Wait up to 30 minutes for background sync
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
                                                        // Optional: could send verbose debug progress events,
                                                        // but main operation is already done.
                                                        // Just trace it.
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

                                    // If not yet Primary/UpToDate (e.g. inconsistent), report progress
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

    // 4. Configure Service Overrides and determine startup services
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
        HaType::Nfs => {
            state.send_progress(
                &operation_id,
                "create_ha_profile",
                Some(&req.name),
                30,
                "Generating service overrides...",
                false,
                None,
            );

            // Generate service overrides for NFS
            if let Some(mount_unit) = &generated_units.mount_unit {
                let override_infos = ServiceOverrideGenerator::generate_for_services(
                    &vec!["nfs-server.service".to_string()],
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
                messages.push("Generated service override(s) for nfs-server".to_string());
            }

            // Use the cached OCF exportfs config from the first pass
            let ocf_exportfs = ocf_exportfs_cache.ok_or_else(|| {
                AppError::Validation("OCF Exportfs configuration not generated".to_string())
            })?;

            // Correct startup order for NFS HA: VIP -> Filesystem -> nfs-server -> exportfs
            vec!["nfs-server.service".to_string(), ocf_exportfs]
        }
        HaType::Iscsi => {
            vec!["target.service".to_string()]
        }
        HaType::NvmeOf => {
            vec![]
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
        nfs: req.nfs.clone(),
        iscsi: req.iscsi.clone(),
        nvmeof: req.nvmeof.clone(),
    };

    let config_gen = ConfigGenerator::new()?;
    let mut profile_for_gen = profile.clone();
    profile_for_gen.promoter.services = _start_services;

    let promoter_config = ConfigGenerator::promoter_from_profile(&profile_for_gen);
    let config_content = config_gen.generate_promoter(&promoter_config)?;
    let config_path = ConfigPaths::promoter_path(&req.name);

    let config_dir = std::path::Path::new(&config_path).parent().unwrap();
    if !config_dir.exists() {
        tokio::fs::create_dir_all(config_dir)
            .await
            .map_err(|e| AppError::Config(format!("Failed to create config dir: {}", e)))?;
    }

    tokio::fs::write(&config_path, &config_content)
        .await
        .map_err(|e| AppError::Config(format!("Failed to write promoter config: {}", e)))?;
    messages.push("Generated promoter configuration".to_string());

    state.send_progress(
        &operation_id,
        "create_ha_profile",
        Some(&req.name),
        60,
        "Configuring managed services...",
        false,
        None,
    );

    let services_to_disable = match req.ha_type {
        HaType::Generic => req.services.clone(),
        HaType::Nfs => vec!["nfs-server.service".to_string()],
        _ => vec![],
    };

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
            state.db.clone(),
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
        "Saving to database...",
        false,
        None,
    );

    state.db.insert_ha_profile(&profile)?;

    // Step 5: Restart drbd-reactor to apply new configuration on all nodes
    state.send_progress(
        &operation_id,
        "create_ha_profile",
        Some(&req.name),
        95,
        "Restarting drbd-reactor on all nodes to apply configuration...",
        false,
        None,
    );

    // Get all nodes for drbd-reactor restart
    let all_nodes = state.db.get_all_nodes()?;
    let mut successful_restarts = 0;
    let mut total_nodes = 0;

    info!("Restarting drbd-reactor service on all nodes to apply new HA configuration");

    // Restart drbd-reactor on each node
    for node in &all_nodes {
        total_nodes += 1;
        let hostname = &node.hostname;

        if node.is_local {
            // Local node restart
            let sys = SystemdController::new().await;
            if let Ok(sys) = sys {
                match sys.restart("drbd-reactor.service").await {
                    Ok(()) => {
                        info!("Successfully restarted drbd-reactor on local node");
                        successful_restarts += 1;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to restart drbd-reactor on local node: {}", e);
                    }
                }
            } else {
                tracing::warn!(
                    "Failed to initialize systemd controller for local drbd-reactor restart"
                );
            }
        } else {
            // Remote node restart via SSH
            let cred = crate::core::SshCredential::Password("ignored".to_string());
            let restart_cmd = "sudo systemctl restart drbd-reactor.service";

            match state
                .ssh_manager
                .execute(&node.ip, node.ssh_port, &node.ssh_user, &cred, restart_cmd)
                .await
            {
                Ok(output) => {
                    if output.success() {
                        info!(
                            "Successfully restarted drbd-reactor on remote node: {}",
                            hostname
                        );
                        successful_restarts += 1;
                    } else {
                        tracing::warn!(
                            "Failed to restart drbd-reactor on remote node {}: {}",
                            hostname,
                            output.stderr
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to connect to remote node {} for drbd-reactor restart: {}",
                        hostname,
                        e
                    );
                }
            }
        }
    }

    // Update messages based on restart results
    if successful_restarts == total_nodes {
        info!(
            "Successfully restarted drbd-reactor on all {}/{} nodes",
            successful_restarts, total_nodes
        );
        messages.push(format!(
            "HA profile created and drbd-reactor restarted on all {} nodes",
            successful_restarts
        ));

        // Give drbd-reactor a moment to start up and read the new configuration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    } else {
        tracing::warn!(
            "drbd-reactor restart succeeded on only {}/{} nodes",
            successful_restarts,
            total_nodes
        );
        messages.push(format!(
            "HA profile created but drbd-reactor restart failed on {}/{} nodes",
            total_nodes - successful_restarts,
            total_nodes
        ));
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
        &format!("HA profile '{}' created successfully", req.name),
    );

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
        }),
    ))
}

/// DELETE /api/v1/ha/profiles/:id
pub async fn delete_profile(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
    Query(_query): Query<DeleteProfileQuery>,
) -> AppResult<StatusCode> {
    let profile = state
        .db
        .get_ha_profile(&id_or_name)?
        .or(state.db.get_ha_profile_by_name(&id_or_name)?)
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

    // Check DRBD status to determine which node is Primary
    let status_cmd = format!("drbdadm status {}", profile.resource_name);
    let status_output = run_shell_command(
        &status_cmd,
        &format!("Check DRBD status for {}", profile.resource_name),
    )
    .await;

    let mut local_primary = false;
    let mut remote_primary_node: Option<String> = None;

    tracing::info!(
        "delete_profile: DRBD status for {}: {}",
        profile.resource_name,
        status_output
            .as_ref()
            .map(|o| &o.stdout)
            .unwrap_or(&"Failed to get status".to_string())
    );

    if let Ok(output) = status_output {
        let status_str = output.stdout;

        // Parse DRBD status to find Primary role
        if status_str.contains("role:Primary") {
            local_primary = true;
            tracing::info!(
                "delete_profile: Local node is Primary for resource {}",
                profile.resource_name
            );
        } else {
            // Look for remote Primary in connections
            for line in status_str.lines() {
                if line.contains("role:Primary") && !line.contains(&profile.resource_name) {
                    // Extract node name from the line (format is usually "nodename role:Primary")
                    if let Some(node_name) = line.split_whitespace().next() {
                        remote_primary_node = Some(node_name.to_string());
                        tracing::info!(
                            "delete_profile: Remote node {} is Primary for resource {}",
                            node_name,
                            profile.resource_name
                        );
                        break;
                    }
                }
            }
        }
    }

    // If no Primary found, we'll assume this node should handle cleanup
    if !local_primary && remote_primary_node.is_none() {
        local_primary = true;
        tracing::warn!("delete_profile: No Primary role found, assuming local node should handle cleanup for {}", profile.resource_name);
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

        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            15,
            "Stopping services...",
            false,
            None,
        );
        let systemd = SystemdController::new().await?;

        // First, completely disable drbd-reactor control for this profile
        let disable_reactor_cmd = format!(
            "drbd-reactorctl disable {} 2>/dev/null || true",
            profile.name
        );
        tracing::info!(
            "delete_profile: Disabling drbd-reactor control for profile '{}'",
            profile.name
        );
        if let Ok(output) = run_shell_command(
            &disable_reactor_cmd,
            &format!("Disable drbd-reactor control for {}", profile.name),
        )
        .await
        {
            tracing::info!(
                "delete_profile: drbd-reactor disable result: success={}, stdout={}, stderr={}",
                output.success(),
                output.stdout,
                output.stderr
            );
        }

        // Stop the drbd-reactor target for this profile
        let stop_reactor_target_cmd = format!(
            "systemctl stop drbd-services@{}.target 2>/dev/null || true",
            profile.name
        );
        tracing::info!(
            "delete_profile: Stopping drbd-reactor target for profile '{}'",
            profile.name
        );
        if let Ok(output) = run_shell_command(
            &stop_reactor_target_cmd,
            &format!("Stop drbd-reactor target for {}", profile.name),
        )
        .await
        {
            tracing::info!(
                "delete_profile: drbd-reactor target stop result: success={}, stdout={}, stderr={}",
                output.success(),
                output.stdout,
                output.stderr
            );
        }

        // First, disable the services to prevent them from being restarted by drbd-reactor
        for service in profile.promoter.services.iter() {
            tracing::info!("delete_profile: Disabling service '{}'", service);
            if let Err(e) = systemd.disable(service).await {
                tracing::warn!("Failed to disable service {}: {}", service, e);
            } else {
                tracing::info!("Successfully disabled service: {}", service);
            }
        }

        // Wait a moment for drbd-reactor to recognize the disable
        tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

        // Then stop the services (both directly managed and drbd-reactor controlled)
        for service in profile.promoter.services.iter().rev() {
            tracing::info!("delete_profile: Stopping service '{}'", service);

            // Try multiple approaches to stop the service
            let mut stopped = false;

            // Try normal stop first
            if let Ok(_) = systemd.stop(service).await {
                tracing::info!("Stopped service via systemctl: {}", service);
                stopped = true;
            } else {
                tracing::warn!(
                    "Failed to stop service {} via systemctl, trying alternative methods",
                    service
                );

                // Try stopping drbd-reactor controlled service directly
                let reactor_stop_cmd = format!(
                    "systemctl stop drbd-services@{}.target 2>/dev/null || true",
                    profile.name
                );
                if let Ok(output) = run_shell_command(
                    &reactor_stop_cmd,
                    &format!("Stop drbd-reactor target for {}", service),
                )
                .await
                {
                    if output.success() {
                        tracing::info!("Stopped drbd-reactor target for service: {}", service);
                        stopped = true;
                    }
                }
            }

            if !stopped {
                tracing::warn!("Failed to stop service: {}", service);
            } else {
                tracing::info!("Successfully stopped service: {}", service);
            }
        }

        // Force stop specific services that might be stubborn
        let force_stop_cmds = vec![
            "systemctl stop postgresql 2>/dev/null || true",
            "systemctl stop mysql 2>/dev/null || true",
            "systemctl stop mariadb 2>/dev/null || true",
        ];

        for cmd in force_stop_cmds {
            if let Ok(output) = run_shell_command(cmd, "Force stop database services").await {
                if !output.stderr.trim().is_empty() {
                    tracing::info!(
                        "Force stop command result: {} - {}",
                        cmd,
                        output.stderr.trim()
                    );
                }
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            18,
            "Disabling drbd-reactor control...",
            false,
            None,
        );

        // Disable the profile in drbd-reactor to prevent restart
        let disable_cmd = format!(
            "drbd-reactorctl disable {} 2>/dev/null || true",
            profile.name
        );
        let _ = run_shell_command(
            &disable_cmd,
            &format!("Disable drbd-reactor control for {}", profile.name),
        )
        .await;

        // Kill processes using the mount point
        let my_pid = std::process::id();
        let kill_cmd = format!(
            "for pid in $(lsof -t {} 2>/dev/null | grep -v {}); do kill -TERM $pid 2>/dev/null || true; done",
            profile.mount_point,
            my_pid
        );
        if let Err(e) = run_shell_command(
            &kill_cmd,
            &format!("Kill processes using mount point {}", profile.mount_point),
        )
        .await
        {
            tracing::warn!("Failed to kill processes using mount point: {}", e);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            22,
            "Killing processes using mount point...",
            false,
            None,
        );

        // Force kill any remaining processes
        let force_kill_cmd = format!(
            "for pid in $(lsof -t {} 2>/dev/null | grep -v {}); do kill -KILL $pid 2>/dev/null || true; done",
            profile.mount_point,
            my_pid
        );
        if let Err(e) = run_shell_command(
            &force_kill_cmd,
            &format!(
                "Force kill remaining processes using mount point {}",
                profile.mount_point
            ),
        )
        .await
        {
            tracing::warn!("Failed to force kill processes using mount point: {}", e);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            25,
            "Unmounting...",
            false,
            None,
        );

        // Check if mount point is actually mounted
        let mount_check_cmd = format!(
            "findmnt -n -o SOURCE {} 2>/dev/null || echo 'Not mounted'",
            profile.mount_point
        );
        if let Ok(mount_check_output) =
            run_shell_command(&mount_check_cmd, "Check if mount point is mounted").await
        {
            if !mount_check_output.stdout.contains("Not mounted") {
                tracing::info!(
                    "delete_profile: Mount point {} is mounted, attempting unmount",
                    profile.mount_point
                );

                // Try multiple unmount methods
                let umount_attempts = vec![
                    format!("umount {}", profile.mount_point),
                    format!("umount -f {}", profile.mount_point),
                    format!("umount -l {}", profile.mount_point),
                ];

                let mut unmounted = false;
                for (i, umount_cmd) in umount_attempts.iter().enumerate() {
                    tracing::info!("delete_profile: Unmount attempt {}: {}", i + 1, umount_cmd);
                    if let Ok(umount_result) = run_shell_command(
                        umount_cmd,
                        &format!("Unmount attempt {} for {}", i + 1, profile.mount_point),
                    )
                    .await
                    {
                        if umount_result.success() {
                            tracing::info!(
                                "delete_profile: Successfully unmounted using: {}",
                                umount_cmd
                            );
                            unmounted = true;
                            break;
                        } else {
                            tracing::warn!(
                                "delete_profile: Unmount attempt {} failed: {}",
                                i + 1,
                                umount_result.stderr.trim()
                            );
                        }
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                }

                if !unmounted {
                    tracing::error!(
                        "delete_profile: All unmount attempts failed for {}",
                        profile.mount_point
                    );
                }
            } else {
                tracing::info!(
                    "delete_profile: Mount point {} is not mounted",
                    profile.mount_point
                );
            }
        }

        // Verify unmount was successful
        tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
        if let Ok(verify_output) = run_shell_command(
            &format!(
                "findmnt -n -o SOURCE {} 2>/dev/null || echo 'Not mounted'",
                profile.mount_point
            ),
            "Verify unmount",
        )
        .await
        {
            if verify_output.stdout.contains("Not mounted") {
                tracing::info!(
                    "delete_profile: Mount point {} successfully unmounted",
                    profile.mount_point
                );
            } else {
                tracing::error!(
                    "delete_profile: Mount point {} is still mounted: {}",
                    profile.mount_point,
                    verify_output.stdout.trim()
                );
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            28,
            "Demoting DRBD resource...",
            false,
            None,
        );
        let demote_cmd = format!("drbdadm secondary {}", profile.resource_name);
        if let Err(e) = run_shell_command(
            &demote_cmd,
            &format!("Demote DRBD resource {}", profile.resource_name),
        )
        .await
        {
            tracing::warn!(
                "Failed to demote DRBD resource {}: {}",
                profile.resource_name,
                e
            );
        } else {
            tracing::info!(
                "Successfully demoted DRBD resource: {}",
                profile.resource_name
            );
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            30,
            "Bringing down DRBD resource...",
            false,
            None,
        );

        // Bring down the DRBD resource
        if let Ok(down_cmd) = DrbdCmd::down_cmd(&profile.resource_name) {
            if let Err(e) = run_shell_command(
                &down_cmd,
                &format!("Bring down DRBD resource {}", profile.resource_name),
            )
            .await
            {
                tracing::warn!(
                    "Failed to bring down DRBD resource {}: {}",
                    profile.resource_name,
                    e
                );
            } else {
                tracing::info!(
                    "Successfully brought down DRBD resource: {}",
                    profile.resource_name
                );
            }
        }

        // Refresh local block devices after DRBD changes
        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            32,
            "Refreshing block devices...",
            false,
            None,
        );

        let local_device_refresh_cmds = vec![
            "sudo partprobe 2>/dev/null || true",
            "sudo udevadm settle --timeout=10 2>/dev/null || true",
            "sudo udevadm trigger --subsystem-match=block --action=add 2>/dev/null || true",
            "sudo systemctl daemon-reload 2>/dev/null || true",
        ];

        for (i, cmd) in local_device_refresh_cmds.iter().enumerate() {
            if let Err(e) =
                run_shell_command(cmd, &format!("Local device refresh command {}", i + 1)).await
            {
                tracing::warn!("Local device refresh command {} failed: {}", i + 1, e);
            } else {
                tracing::info!(
                    "Local device refresh command {} executed successfully",
                    i + 1
                );
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        // Verify local device cleanup
        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            33,
            "Verifying local device cleanup...",
            false,
            None,
        );

        let local_verify_cmd = format!(
            "lsblk | grep {} || echo 'Device cleaned up'",
            profile.resource_name
        );
        if let Ok(output) = run_shell_command(&local_verify_cmd, "Local device verification").await
        {
            tracing::info!("Local device verification: {}", output.stdout);
            if output.stdout.contains("drbd") || output.stderr.contains("drbd") {
                tracing::warn!("DRBD device still found locally: {}", output.stdout);
            } else {
                tracing::info!("DRBD devices successfully cleaned up locally");
            }
        }
    } else if let Some(node_name) = remote_primary_node {
        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            10,
            &format!("Deactivating active profile on {}...", node_name),
            false,
            None,
        );

        let nodes = state.db.get_all_nodes()?;
        if let Some(node) = nodes
            .iter()
            .find(|n| n.hostname == node_name || n.id == node_name)
        {
            if let Ok(Some(credential)) = get_node_credential(&state, node).await {
                if let Some(vip) = &profile.vip {
                    let vip_cmd = format!(
                        "sudo ip addr del {}/{} dev {} 2>/dev/null || true",
                        vip.address, vip.netmask, vip.interface
                    );
                    let _ = state
                        .ssh_manager
                        .execute(
                            &node.ip,
                            node.ssh_port,
                            &node.ssh_user,
                            &credential,
                            &vip_cmd,
                        )
                        .await;
                }

                state.send_progress(
                    &operation_id,
                    "delete_ha_profile",
                    Some(&profile.name),
                    15,
                    "Disabling drbd-reactor control...",
                    false,
                    None,
                );

                // Disable the profile in drbd-reactor to prevent restart
                let disable_reactor_cmd = format!(
                    "sudo drbd-reactorctl disable {} 2>/dev/null || true",
                    profile.name
                );
                let disable_result = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &disable_reactor_cmd,
                    )
                    .await;

                match &disable_result {
                    Ok(output) => {
                        tracing::info!("drbd-reactor disable command executed on {}, success: {}, stdout: {}, stderr: {}",
                            node.hostname, output.success(), output.stdout, output.stderr);
                        if output.success() {
                            tracing::info!(
                                "Successfully disabled drbd-reactor on {}",
                                node.hostname
                            );
                        } else {
                            tracing::warn!(
                                "drbd-reactor disable failed on {}: {}",
                                node.hostname,
                                output.stderr
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to execute drbd-reactor disable command on {}: {}",
                            node.hostname,
                            e
                        );
                    }
                }

                state.send_progress(
                    &operation_id,
                    "delete_ha_profile",
                    Some(&profile.name),
                    18,
                    "Disabling services...",
                    false,
                    None,
                );

                // First disable the services to prevent them from being restarted
                for service in profile.promoter.services.iter() {
                    let disable_cmd = format!("sudo systemctl disable {}", service);
                    if let Err(e) = state
                        .ssh_manager
                        .execute(
                            &node.ip,
                            node.ssh_port,
                            &node.ssh_user,
                            &credential,
                            &disable_cmd,
                        )
                        .await
                    {
                        tracing::warn!(
                            "Failed to disable service {} on {}: {}",
                            service,
                            node.hostname,
                            e
                        );
                    } else {
                        tracing::info!(
                            "Successfully disabled service {} on {}",
                            service,
                            node.hostname
                        );
                    }
                }

                state.send_progress(
                    &operation_id,
                    "delete_ha_profile",
                    Some(&profile.name),
                    20,
                    "Stopping services...",
                    false,
                    None,
                );

                // Stop the drbd-reactor target first
                let stop_reactor_svc_cmd = format!(
                    "sudo systemctl stop drbd-services@{}.target 2>/dev/null || true",
                    profile.name
                );
                let reactor_stop_result = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &stop_reactor_svc_cmd,
                    )
                    .await;

                match &reactor_stop_result {
                    Ok(output) => {
                        tracing::info!("drbd-reactor target stop command executed on {}, success: {}, stdout: {}, stderr: {}",
                            node.hostname, output.success(), output.stdout, output.stderr);
                        if output.success() {
                            tracing::info!(
                                "Successfully stopped drbd-reactor target on {}",
                                node.hostname
                            );
                        } else {
                            tracing::warn!(
                                "drbd-reactor target stop failed on {}: {}",
                                node.hostname,
                                output.stderr
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to execute drbd-reactor target stop command on {}: {}",
                            node.hostname,
                            e
                        );
                    }
                }

                // Then stop all services
                for service in profile.promoter.services.iter().rev() {
                    let stop_cmd = format!("sudo systemctl stop {}", service);
                    let service_stop_result = state
                        .ssh_manager
                        .execute(
                            &node.ip,
                            node.ssh_port,
                            &node.ssh_user,
                            &credential,
                            &stop_cmd,
                        )
                        .await;

                    match &service_stop_result {
                        Ok(output) => {
                            tracing::info!("systemctl stop {} command executed on {}, success: {}, stdout: {}, stderr: {}",
                                service, node.hostname, output.success(), output.stdout, output.stderr);
                            if output.success() {
                                tracing::info!(
                                    "Successfully stopped service {} on {}",
                                    service,
                                    node.hostname
                                );
                            } else {
                                tracing::warn!(
                                    "Failed to stop service {} on {}: {}",
                                    service,
                                    node.hostname,
                                    output.stderr
                                );
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to execute systemctl stop {} command on {}: {}",
                                service,
                                node.hostname,
                                e
                            );
                        }
                    }

                    // Add small delay between service stops
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

                state.send_progress(
                    &operation_id,
                    "delete_ha_profile",
                    Some(&profile.name),
                    21,
                    "Force stopping remaining services...",
                    false,
                    None,
                );

                // Force stop all services that might still be running
                let force_stop_cmd = format!("sudo systemctl stop mysql 2>/dev/null || true");
                let force_stop_result = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &force_stop_cmd,
                    )
                    .await;

                match &force_stop_result {
                    Ok(output) => {
                        tracing::info!("Force stop mysql command executed on {}, success: {}, stdout: {}, stderr: {}",
                            node.hostname, output.success(), output.stdout, output.stderr);
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to execute force stop mysql command on {}: {}",
                            node.hostname,
                            e
                        );
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

                state.send_progress(
                    &operation_id,
                    "delete_ha_profile",
                    Some(&profile.name),
                    22,
                    "Killing processes using mount point...",
                    false,
                    None,
                );

                // Kill processes using the mount point
                let kill_cmd = format!(
                    "for pid in $(sudo lsof -t {} 2>/dev/null); do sudo kill -TERM $pid 2>/dev/null || true; done",
                    profile.mount_point
                );
                if let Err(e) = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &kill_cmd,
                    )
                    .await
                {
                    tracing::warn!(
                        "Failed to kill processes using mount point on {}: {}",
                        node.hostname,
                        e
                    );
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

                // Force kill any remaining processes
                let force_kill_cmd = format!(
                    "for pid in $(sudo lsof -t {} 2>/dev/null); do sudo kill -KILL $pid 2>/dev/null || true; done",
                    profile.mount_point
                );
                if let Err(e) = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &force_kill_cmd,
                    )
                    .await
                {
                    tracing::warn!("Failed to force kill processes on {}: {}", node.hostname, e);
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                state.send_progress(
                    &operation_id,
                    "delete_ha_profile",
                    Some(&profile.name),
                    25,
                    "Unmounting...",
                    false,
                    None,
                );

                // Try regular unmount first
                let umount_cmd = format!("sudo umount {}", profile.mount_point);
                let umount_result = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &umount_cmd,
                    )
                    .await;

                // If regular unmount fails, try lazy unmount
                if umount_result.is_err() || umount_result.as_ref().unwrap().success() == false {
                    tracing::warn!(
                        "Regular unmount failed on {}, trying lazy unmount",
                        node.hostname
                    );
                    let lazy_umount_cmd = format!("sudo umount -l {}", profile.mount_point);
                    if let Err(e) = state
                        .ssh_manager
                        .execute(
                            &node.ip,
                            node.ssh_port,
                            &node.ssh_user,
                            &credential,
                            &lazy_umount_cmd,
                        )
                        .await
                    {
                        tracing::error!(
                            "Failed to unmount {} on {}: {}",
                            profile.mount_point,
                            node.hostname,
                            e
                        );
                    } else {
                        tracing::info!(
                            "Successfully lazy unmounted {} on {}",
                            profile.mount_point,
                            node.hostname
                        );
                    }
                } else {
                    tracing::info!(
                        "Successfully unmounted {} on {}",
                        profile.mount_point,
                        node.hostname
                    );
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

                state.send_progress(
                    &operation_id,
                    "delete_ha_profile",
                    Some(&profile.name),
                    28,
                    "Demoting DRBD resource...",
                    false,
                    None,
                );
                let demote_cmd = format!("sudo drbdadm secondary {}", profile.resource_name);
                if let Err(e) = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &demote_cmd,
                    )
                    .await
                {
                    tracing::warn!(
                        "Failed to demote DRBD resource {} on {}: {}",
                        profile.resource_name,
                        node.hostname,
                        e
                    );
                } else {
                    tracing::info!(
                        "Successfully demoted DRBD resource {} on {}",
                        profile.resource_name,
                        node.hostname
                    );
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                state.send_progress(
                    &operation_id,
                    "delete_ha_profile",
                    Some(&profile.name),
                    30,
                    "Bringing down DRBD resource...",
                    false,
                    None,
                );

                // Bring down the DRBD resource on remote node
                if let Ok(down_cmd) = DrbdCmd::down_cmd(&profile.resource_name) {
                    if let Err(e) = state
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
                        tracing::warn!(
                            "Failed to bring down DRBD resource {} on {}: {}",
                            profile.resource_name,
                            node.hostname,
                            e
                        );
                    } else {
                        tracing::info!(
                            "Successfully brought down DRBD resource {} on {}",
                            profile.resource_name,
                            node.hostname
                        );
                    }
                }

                state.send_progress(
                    &operation_id,
                    "delete_ha_profile",
                    Some(&profile.name),
                    32,
                    "Refreshing block devices...",
                    false,
                    None,
                );

                // Refresh block device information after DRBD changes
                let device_refresh_cmds = vec![
                    "sudo partprobe 2>/dev/null || true",
                    "sudo udevadm settle --timeout=10 2>/dev/null || true",
                    "sudo udevadm trigger --subsystem-match=block --action=add 2>/dev/null || true",
                    "sudo systemctl daemon-reload 2>/dev/null || true",
                ];

                for (i, cmd) in device_refresh_cmds.iter().enumerate() {
                    if let Err(e) = state
                        .ssh_manager
                        .execute(&node.ip, node.ssh_port, &node.ssh_user, &credential, cmd)
                        .await
                    {
                        tracing::warn!(
                            "Device refresh command {} failed on {}: {}",
                            i + 1,
                            node.hostname,
                            e
                        );
                    } else {
                        tracing::info!(
                            "Device refresh command {} executed successfully on {}",
                            i + 1,
                            node.hostname
                        );
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                }

                state.send_progress(
                    &operation_id,
                    "delete_ha_profile",
                    Some(&profile.name),
                    33,
                    "Verifying device cleanup...",
                    false,
                    None,
                );

                // Verify that DRBD devices are cleaned up
                let verify_cmd = format!(
                    "sudo lsblk | grep {} || echo 'Device cleaned up'",
                    profile.resource_name
                );
                if let Ok(output) = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &verify_cmd,
                    )
                    .await
                {
                    tracing::info!(
                        "Device verification on {}: {}",
                        node.hostname,
                        output.stdout
                    );
                    if output.stdout.contains("drbd") || output.stderr.contains("drbd") {
                        tracing::warn!(
                            "DRBD device still found on {}: {}",
                            node.hostname,
                            output.stdout
                        );
                    } else {
                        tracing::info!("DRBD devices successfully cleaned up on {}", node.hostname);
                    }
                }
            }
        }
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        30,
        "Removing service overrides...",
        false,
        None,
    );
    let service_names: Vec<String> = profile.promoter.services.clone();
    let _ = ServiceOverrideGenerator::remove_for_services(&service_names).await;

    let _ = MountUnitGenerator::remove(&profile.mount_point).await;

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        40,
        "Removing promoter configuration...",
        false,
        None,
    );

    let config_path = ConfigPaths::promoter_path(&profile.name);

    if tokio::fs::metadata(&config_path).await.is_ok() {
        let _ = tokio::fs::remove_file(&config_path).await;
    }

    // Step 4: Reload systemd daemon and restart drbd-reactor
    if let Ok(sys) = SystemdController::new().await {
        let _ = sys.daemon_reload().await;

        // Restart drbd-reactor to ensure it picks up all changes
        tracing::info!("delete_profile: Restarting drbd-reactor service");
        if let Err(e) = sys.restart("drbd-reactor.service").await {
            tracing::warn!("Failed to restart drbd-reactor service: {}", e);
        } else {
            tracing::info!("Successfully restarted drbd-reactor service");
        }

        // Give drbd-reactor a moment to start up
        tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        50,
        "Cleaning up remote nodes...",
        false,
        None,
    );
    let sync_config = HaSyncConfig {
        drbd_resource_config: profile
            .generated_units
            .drbd_config_path
            .clone()
            .map(|p| (p, String::new())),
        mount_unit: profile
            .generated_units
            .mount_unit_path
            .clone()
            .map(|p| (p, String::new())),
        service_overrides: profile
            .generated_units
            .service_overrides
            .iter()
            .map(|o| (o.override_path.clone(), String::new()))
            .collect(),
        promoter_config: (config_path.clone(), String::new()),
    };
    let cluster_sync = ClusterSync::new(
        state.ssh_manager.clone(),
        state.db.clone(),
        state.credentials.clone(),
    );
    let synced_nodes = cluster_sync.remove_ha_config(&sync_config).await?;

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        60,
        "Restarting drbd-reactor on all nodes...",
        false,
        None,
    );

    // Step 4: Restart drbd-reactor on all nodes to apply configuration changes
    let nodes = state.db.get_all_nodes()?;
    let mut successful_restarts = 0;
    let mut total_nodes = 0;

    info!("Restarting drbd-reactor service on all nodes to apply HA profile deletion");

    for node in &nodes {
        total_nodes += 1;
        let hostname = &node.hostname;

        if node.is_local {
            // Local node restart
            let sys = SystemdController::new().await;
            if let Ok(sys) = sys {
                if let Err(e) = sys.daemon_reload().await {
                    tracing::warn!("Failed to reload systemd daemon on local node: {}", e);
                }

                match sys.restart("drbd-reactor.service").await {
                    Ok(()) => {
                        info!("Successfully restarted drbd-reactor on local node");
                        successful_restarts += 1;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to restart drbd-reactor on local node: {}", e);
                    }
                }
            } else {
                tracing::warn!(
                    "Failed to initialize systemd controller for local drbd-reactor restart"
                );
            }
        } else if synced_nodes.contains(&node.hostname) {
            // Remote node restart via SSH (only if config sync was successful)
            let cred = crate::core::SshCredential::Password("ignored".to_string());

            // Reload systemd daemon and restart drbd-reactor
            let commands = vec![
                "sudo systemctl daemon-reload",
                "sudo systemctl restart drbd-reactor.service",
            ];

            let mut restart_success = true;
            for cmd in commands {
                match state
                    .ssh_manager
                    .execute(&node.ip, node.ssh_port, &node.ssh_user, &cred, cmd)
                    .await
                {
                    Ok(output) => {
                        if !output.success() {
                            tracing::warn!(
                                "Failed to execute '{}' on remote node {}: {}",
                                cmd,
                                hostname,
                                output.stderr
                            );
                            restart_success = false;
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to connect to remote node {} for command '{}': {}",
                            hostname,
                            cmd,
                            e
                        );
                        restart_success = false;
                    }
                }
            }

            if restart_success {
                info!(
                    "Successfully restarted drbd-reactor on remote node: {}",
                    hostname
                );
                successful_restarts += 1;
            } else {
                tracing::warn!(
                    "Failed to restart drbd-reactor on remote node: {}",
                    hostname
                );
            }
        }
    }

    // Give drbd-reactor a moment to start up and read the updated configuration
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    info!(
        "drbd-reactor restart completed: {}/{} nodes successful",
        successful_restarts, total_nodes
    );

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        70,
        "Removing from database...",
        false,
        None,
    );
    state.db.delete_ha_profile(&profile.id)?;

    {
        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            80,
            "Deleting DRBD resource...",
            false,
            None,
        );

        // Bring down the DRBD resource (in case it wasn't brought down earlier)
        if let Ok(down_cmd) = DrbdCmd::down_cmd(&resource_name) {
            if let Err(e) = run_shell_command(
                &down_cmd,
                &format!("Bring down DRBD resource {}", resource_name),
            )
            .await
            {
                tracing::warn!(
                    "Failed to bring down DRBD resource {}: {}",
                    resource_name,
                    e
                );
            } else {
                tracing::info!("Successfully brought down DRBD resource: {}", resource_name);
            }
        }

        // Remove DRBD configuration file
        let drbd_config_path = ConfigPaths::drbd_resource_path(&resource_name);
        if tokio::fs::metadata(&drbd_config_path).await.is_ok() {
            if let Err(e) = tokio::fs::remove_file(&drbd_config_path).await {
                tracing::warn!(
                    "Failed to remove DRBD config file {}: {}",
                    drbd_config_path,
                    e
                );
            } else {
                tracing::info!(
                    "Successfully removed DRBD config file: {}",
                    drbd_config_path
                );
            }
        }

        // Remove DRBD resource from all nodes
        if let Err(e) = cluster_sync.remove_drbd_resource(&resource_name).await {
            tracing::warn!("Failed to sync DRBD resource removal to cluster: {}", e);
        } else {
            tracing::info!("Successfully synced DRBD resource removal to cluster");
        }
    }

    if let Some(volume) = state.db.get_volume_by_drbd_res(&resource_name)? {
        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            90,
            "Deleting LVM volume...",
            false,
            None,
        );
        if let Ok(Some(storage_pool)) = state.db.get_storage_pool(&volume.pool_id) {
            let all_nodes = state.db.get_all_nodes()?;
            for node in &all_nodes {
                let lvm_provider = if node.is_local {
                    LvmProvider::new_local(storage_pool.name.clone())
                } else if let Ok(Some(credential)) = get_node_credential(&state, node).await {
                    LvmProvider::new_remote(
                        storage_pool.name.clone(),
                        state.ssh_manager.clone(),
                        node.ip.clone(),
                        node.ssh_port,
                        node.ssh_user.clone(),
                        credential,
                    )
                } else {
                    continue;
                };

                let _ = lvm_provider.delete_volume(&volume.name).await;
            }

            if let Ok(Some(vg_info)) = crate::core::lvm_utils::get_vg_info(&storage_pool.name).await
            {
                let _ = state.db.update_storage_pool_sizes(
                    &storage_pool.id,
                    vg_info.size,
                    vg_info.free,
                );
            }
        }

        state.db.delete_volume(&volume.id)?;
    }

    state.send_progress(
        &operation_id,
        "delete_ha_profile",
        Some(&profile.name),
        100,
        "Deletion completed successfully",
        true,
        Some(true),
    );
    state.send_notification(
        NotificationLevel::Success,
        "HA Profile Deleted",
        &format!("HA profile '{}' deleted successfully", profile.name),
    );

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/ha/profiles/:id/status
pub async fn get_profile_status(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
) -> AppResult<Json<HaProfileDetailResponse>> {
    fetch_profile_details(state, id_or_name).await
}

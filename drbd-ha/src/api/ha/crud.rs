use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use drbd_migration::{DataMigration, MigrationConfig};
use drbd_reactor_utils::DrbdReactorClient;
use drbd_utils::parse_drbdadm_status;
use std::sync::Arc;

use crate::core::{
    cluster_sync::{ClusterSync, HaSyncConfig},
    config_gen::{ConfigGenerator, ConfigPaths, NodeConfig, ResourceConfig},
    drbd_cmd,
    mount_unit::MountUnitGenerator,
    run_shell_command,
    service_override::ServiceOverrideGenerator,
    systemd_ctrl::SystemdController,
    validator, IscsiGenerator, LvmProvider, NfsGenerator, NvmeOfGenerator, ServiceInitFactory,
    StorageProvider,
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
    Json(req): Json<CreateHaProfileRequest>,
) -> AppResult<(StatusCode, Json<HaProfileCreateResponse>)> {
    let operation_id = uuid::Uuid::new_v4().to_string();

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
    }

    if let Some(vip) = &req.vip {
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

        let drbd_port = req.drbd_port.unwrap_or(7789);
        let drbd_minor = req.drbd_minor.unwrap_or(0);

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

        let remote_nodes: Vec<_> = all_nodes.iter().filter(|n| !n.is_local).collect();
        for node in &remote_nodes {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            state
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

    let mut generated_units = GeneratedUnits::default();
    let mut messages = Vec::new();
    let extra_sync_files: Vec<(String, String)> = Vec::new();

    if matches!(req.ha_type, HaType::Generic | HaType::Nfs) {
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

            if req.lvm_pool_id.is_some() {
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
                let mkfs_cmd = format!(
                    "mkfs.{} /dev/drbd{}",
                    req.fs_type,
                    req.drbd_minor.unwrap_or(0)
                );
                run_shell_command(&mkfs_cmd, "Create filesystem").await?;

                run_shell_command(
                    &format!("mkdir -p {}", req.mount_point),
                    "Create mount point",
                )
                .await?;
                run_shell_command(
                    &format!(
                        "mount /dev/drbd{} {}",
                        req.drbd_minor.unwrap_or(0),
                        req.mount_point
                    ),
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
            }

            req.services.clone()
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

            let services = vec!["nfs-server.service".to_string()];

            if req.lvm_pool_id.is_some() {
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

                let mkfs_cmd = format!(
                    "mkfs.{} /dev/drbd{}",
                    req.fs_type,
                    req.drbd_minor.unwrap_or(0)
                );
                run_shell_command(&mkfs_cmd, "Create filesystem for NFS state").await?;

                run_shell_command(
                    &format!("mkdir -p {}", req.mount_point),
                    "Create mount point",
                )
                .await?;
                run_shell_command(
                    &format!(
                        "mount /dev/drbd{} {}",
                        req.drbd_minor.unwrap_or(0),
                        req.mount_point
                    ),
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
            }

            state.send_progress(
                &operation_id,
                "create_ha_profile",
                Some(&req.name),
                40,
                "Configuring NFS exports...",
                false,
                None,
            );

            let exports_content = format!(
                "{} {}({})
",
                req.mount_point,
                nfs_config
                    .allowed_networks
                    .first()
                    .unwrap_or(&"*".to_string()),
                nfs_config.options
            );

            let mut current_exports = tokio::fs::read_to_string("/etc/exports")
                .await
                .unwrap_or_default();

            current_exports = current_exports
                .lines()
                .filter(|line| !line.starts_with(&format!("{} ", req.mount_point)))
                .collect::<Vec<_>>()
                .join("\n");

            if !current_exports.is_empty() && !current_exports.ends_with('\n') {
                current_exports.push('\n');
            }
            current_exports.push_str(&exports_content);

            tokio::fs::write("/etc/exports", current_exports.as_bytes())
                .await
                .map_err(|e| AppError::Config(format!("Failed to write /etc/exports: {}", e)))?;

            let remote_nodes: Vec<_> = all_nodes.iter().filter(|n| !n.is_local).collect();
            for node in &remote_nodes {
                let credential = crate::core::SshCredential::Password("ignored".to_string());
                state
                    .ssh_manager
                    .write_file(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        "/etc/exports",
                        &current_exports,
                    )
                    .await?;
            }

            services
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
                vec!["target.service".to_string()]
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
                vec![]
            } else {
                return Err(AppError::Validation(
                    "NVMe-oF configuration or VIP missing".to_string(),
                ));
            }
        }
    };

    let migration_result = None;
    if matches!(req.ha_type, HaType::Generic | HaType::Nfs) {
        if let Some(ref migration_opts) = req.migration {
            if migration_opts.migrate_data {
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
                };

                let resource_name = req.resource_name.clone();
                let profile_name = req.name.clone();
                let op_id = operation_id.clone();
                let state_clone = state.clone();

                tokio::spawn(async move {
                    state_clone.send_progress(
                        &op_id,
                        "create_ha_profile",
                        Some(&profile_name),
                        40,
                        "Starting background data migration...",
                        false,
                        None,
                    );

                    match DataMigration::migrate(migration_config, None).await {
                        Ok(result) => {
                            state_clone.send_progress(
                                &op_id,
                                "create_ha_profile",
                                Some(&profile_name),
                                45,
                                &format!(
                                    "Migration done ({} bytes). Syncing DRBD...",
                                    result.bytes_transferred
                                ),
                                false,
                                None,
                            );

                            loop {
                                let cmd = format!("drbdadm status {}", resource_name);
                                if let Ok(output) =
                                    run_shell_command(&cmd, "Check DRBD sync status").await
                                {
                                    if output.success()
                                        && output.stdout.contains("peer-disk:UpToDate")
                                        && !output.stdout.contains("Inconsistent")
                                        && !output.stdout.contains("SyncSource")
                                        && !output.stdout.contains("SyncTarget")
                                    {
                                        state_clone.send_progress(
                                            &op_id,
                                            "create_ha_profile",
                                            Some(&profile_name),
                                            48,
                                            "DRBD resource fully synced",
                                            false,
                                            None,
                                        );
                                        break;
                                    }
                                }
                                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                            }
                        }
                        Err(e) => {
                            state_clone.send_progress(
                                &op_id,
                                "create_ha_profile",
                                Some(&profile_name),
                                40,
                                &format!("Migration failed: {}", e),
                                true,
                                None,
                            );
                        }
                    }
                });

                messages.push("Data migration started in background".to_string());
            }
        }
    }

    state.send_progress(
        &operation_id,
        "create_ha_profile",
        Some(&req.name),
        50,
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
        vip: req.vip.clone(),
        promoter: PromoterSettings {
            services: req.services.clone(),
            stop_on_demote: req.stop_on_demote,
            on_demote_failure: req.on_demote_failure.clone(),
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
        65,
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

        let remote_nodes: Vec<_> = all_nodes.iter().filter(|n| !n.is_local).collect();
        for node in &remote_nodes {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            for service in &services_to_disable {
                let disable_cmd =
                    format!("systemctl disable --now {} 2>/dev/null || true", service);
                let _ = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &disable_cmd,
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

    let _ = run_shell_command("systemctl daemon-reload", "Reload systemd daemon").await;

    state.send_progress(
        &operation_id,
        "create_ha_profile",
        Some(&req.name),
        80,
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

        let sync_config = HaSyncConfig {
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

    messages.push("Reload drbd-reactor to apply".to_string());

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

    let status_cmd = format!(
        "drbdadm status --json {} 2>/dev/null",
        profile.resource_name
    );
    let status_output = run_shell_command(
        &status_cmd,
        &format!("Check DRBD status for {}", profile.resource_name),
    )
    .await;

    let mut local_primary = false;
    let mut remote_primary_node: Option<String> = None;

    if let Ok(output) = status_output {
        if let Ok(statuses) = drbd_cmd::parse_drbd_status(&output.stdout) {
            if let Some(res_status) = statuses.first() {
                if res_status.is_primary() {
                    local_primary = true;
                } else {
                    for conn in &res_status.connections {
                        if conn.peer_role.as_deref() == Some("Primary") {
                            remote_primary_node = Some(conn.name.clone());
                            break;
                        }
                    }
                }
            }
        }
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
        for service in profile.promoter.services.iter().rev() {
            let _ = systemd.stop(service).await;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let my_pid = std::process::id();
        let kill_cmd = format!(
            "for pid in $(lsof -t {} 2>/dev/null | grep -v {}); do kill -9 $pid 2>/dev/null || true; done",
            profile.mount_point,
            my_pid
        );
        let _ = run_shell_command(
            &kill_cmd,
            &format!("Kill processes using mount point {}", profile.mount_point),
        )
        .await;

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            20,
            "Unmounting...",
            false,
            None,
        );
        let umount_cmd = format!(
            "umount {} 2>/dev/null || umount -l {} 2>/dev/null || true",
            profile.mount_point, profile.mount_point
        );
        let _ = run_shell_command(&umount_cmd, &format!("Unmount {}", profile.mount_point)).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        state.send_progress(
            &operation_id,
            "delete_ha_profile",
            Some(&profile.name),
            25,
            "Demoting DRBD resource...",
            false,
            None,
        );
        let demote_cmd = format!("drbdadm secondary {}", profile.resource_name);
        let _ = run_shell_command(
            &demote_cmd,
            &format!("Demote DRBD resource {}", profile.resource_name),
        )
        .await;
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

                let disable_reactor_cmd = format!(
                    "sudo drbd-reactorctl disable {} 2>/dev/null || true",
                    profile.name
                );
                let _ = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &disable_reactor_cmd,
                    )
                    .await;

                state.send_progress(
                    &operation_id,
                    "delete_ha_profile",
                    Some(&profile.name),
                    15,
                    "Stopping services...",
                    false,
                    None,
                );

                let stop_reactor_svc_cmd = format!(
                    "sudo systemctl stop drbd-services@{}.target 2>/dev/null || true",
                    profile.name
                );
                let _ = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &stop_reactor_svc_cmd,
                    )
                    .await;

                for service in profile.promoter.services.iter().rev() {
                    let stop_cmd = format!("sudo systemctl stop {}", service);
                    let _ = state
                        .ssh_manager
                        .execute(
                            &node.ip,
                            node.ssh_port,
                            &node.ssh_user,
                            &credential,
                            &stop_cmd,
                        )
                        .await;
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

                let kill_cmd = format!(
                    "for pid in $(sudo lsof -t {} 2>/dev/null); do sudo kill -9 $pid 2>/dev/null || true; done",
                    profile.mount_point
                );
                let _ = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &kill_cmd,
                    )
                    .await;

                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

                state.send_progress(
                    &operation_id,
                    "delete_ha_profile",
                    Some(&profile.name),
                    20,
                    "Unmounting...",
                    false,
                    None,
                );
                let umount_cmd = format!(
                    "sudo umount {} 2>/dev/null || sudo umount -l {} 2>/dev/null || true",
                    profile.mount_point, profile.mount_point
                );
                let _ = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &umount_cmd,
                    )
                    .await;

                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                state.send_progress(
                    &operation_id,
                    "delete_ha_profile",
                    Some(&profile.name),
                    25,
                    "Demoting DRBD resource...",
                    false,
                    None,
                );
                let demote_cmd = format!("sudo drbdadm secondary {}", profile.resource_name);
                let _ = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        &demote_cmd,
                    )
                    .await;

                let reload_cmd = "sudo systemctl daemon-reload";
                let _ = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &credential,
                        reload_cmd,
                    )
                    .await;
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

    let _ = run_shell_command("systemctl daemon-reload", "Reload systemd daemon").await;

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
    let _ = cluster_sync.remove_ha_config(&sync_config).await;

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
        use crate::core::drbd_cmd::DrbdCmd;

        if let Ok(down_cmd) = DrbdCmd::down_cmd(&resource_name) {
            let _ = run_shell_command(
                &down_cmd,
                &format!("Bring down DRBD resource {}", resource_name),
            )
            .await;
        }

        let drbd_config_path = ConfigPaths::drbd_resource_path(&resource_name);
        if tokio::fs::metadata(&drbd_config_path).await.is_ok() {
            let _ = tokio::fs::remove_file(&drbd_config_path).await;
        }

        let _ = cluster_sync.remove_drbd_resource(&resource_name).await;
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

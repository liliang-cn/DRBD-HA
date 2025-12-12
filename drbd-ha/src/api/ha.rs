//! HA profile management API handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::core::{
    cluster_sync::{ClusterSync, HaSyncConfig},
    config_gen::{ConfigGenerator, ConfigPaths, NodeConfig, ResourceConfig},
    mount_unit::MountUnitGenerator,
    run_shell_command,
    service_override::ServiceOverrideGenerator,
    systemd_ctrl::{ServiceFileInfo, ServiceInfo, SystemdController},
    validator, IscsiGenerator, LvmProvider, NfsGenerator, NvmeOfGenerator, ReactorDiscovery,
    ServiceInitFactory, StorageProvider,
};
use crate::error::{AppError, AppResult};
use crate::models::{
    CreateHaProfileRequest, GeneratedUnits, HaProfile, HaProfileStatus, HaType, Node,
    PromoterSettings, ServiceOverride,
};
use crate::state::{AppState, NotificationLevel};
use axum::extract::Query;
use drbd_migration::{DataMigration, MigrationConfig};

/// Get SSH credential for a node (Dummy) - copied from cluster.rs
async fn get_node_credential(
    _state: &Arc<AppState>,
    _node: &Node,
) -> AppResult<Option<crate::core::SshCredential>> {
    // We don't use credentials anymore, just return a dummy one
    Ok(Some(crate::core::SshCredential::Password(
        "ignored".to_string(),
    )))
}

/// Response for HA profile list
#[derive(Serialize)]
pub struct HaProfileListResponse {
    pub profiles: Vec<HaProfile>,
}

/// Response for HA profile creation
#[derive(Serialize)]
pub struct HaProfileCreateResponse {
    pub profile: HaProfile,
    pub config_path: String,
    pub message: String,
    /// Services that were disabled (if auto_disable_services was true)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub disabled_services: Vec<String>,
    /// Information about generated systemd units
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_units: Option<GeneratedUnits>,
    /// Data migration result (if migration was performed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_result: Option<MigrationResultInfo>,
    /// Nodes that were synced with HA configuration
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub synced_nodes: Vec<String>,
}

/// Summary of data migration result
#[derive(Serialize)]
pub struct MigrationResultInfo {
    pub bytes_transferred: u64,
    pub source_path: String,
    pub services_restarted: Vec<String>,
}

/// Response for HA profile status
#[derive(Serialize)]
pub struct HaProfileStatusResponse {
    pub id: String,
    pub name: String,
    pub status: HaProfileStatus,
    /// Currently active node (from drbd-reactorctl)
    pub active_node: Option<String>,
    /// Detected mount point from mount unit (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_point: Option<String>,
    /// Detailed DRBD resource status
    pub drbd: Option<DrbdResourceStatus>,
    pub service_statuses: Vec<ServiceStatusInfo>,
    pub vip_active: Option<bool>,
    /// Configuration visibility info
    pub config: ConfigVisibility,
    /// Raw output from drbd-reactorctl status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reactor_status_raw: Option<String>,
}

/// Detailed DRBD resource status
#[derive(Serialize)]
pub struct DrbdResourceStatus {
    /// Resource name
    pub resource: String,
    /// Local role (Primary/Secondary)
    pub role: String,
    /// Local disk state (UpToDate/Inconsistent/DUnknown etc)
    pub disk: String,
    /// Whether the device is open (mounted)
    pub open: bool,
    /// Peer node statuses
    pub peers: Vec<DrbdPeerStatus>,
}

/// Status of a DRBD peer node
#[derive(Serialize)]
pub struct DrbdPeerStatus {
    /// Peer hostname
    pub name: String,
    /// Peer role (Primary/Secondary)
    pub role: String,
    /// Peer disk state
    pub peer_disk: String,
    /// Connection state (Connected/Connecting etc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    /// Replication state (Established/SyncSource/SyncTarget etc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replication: Option<String>,
}

/// Configuration visibility information
#[derive(Serialize)]
pub struct ConfigVisibility {
    /// Whether the promoter config file exists
    pub promoter_config_exists: bool,
    /// Path to the promoter config file
    pub promoter_config_path: String,
    /// Whether drbd-reactor service is running
    pub reactor_running: bool,
}

#[derive(Serialize)]
pub struct ServiceStatusInfo {
    pub name: String,
    pub active: bool,
    pub state: String,
    /// Whether the service is enabled (starts on boot)
    pub enabled: bool,
    /// When the service became active (Unix timestamp in seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_since: Option<i64>,
}

/// Parse VIP configuration from drbd-reactor config file content
/// Example: start = ["...", "ocf:heartbeat:IPaddr2 vip cidr_netmask=24 ip=192.168.123.198", "..."]
fn parse_vip_from_config(content: &str) -> Option<crate::models::VipConfig> {
    use crate::models::VipConfig;

    // Find lines containing IPaddr2
    for line in content.lines() {
        if line.contains("ocf:heartbeat:IPaddr2") {
            // Extract the service definition from TOML array (between quotes)
            let service_def = if let Some(start) = line.find("ocf:heartbeat:IPaddr2") {
                // Find the end quote after IPaddr2
                let after_ipaddr2 = &line[start..];
                if let Some(end_quote) = after_ipaddr2.find('"') {
                    &after_ipaddr2[..end_quote]
                } else {
                    after_ipaddr2
                }
            } else {
                continue;
            };

            // Extract ip= and cidr_netmask= parameters
            let mut ip_addr = None;
            let mut netmask = None;

            for part in service_def.split_whitespace() {
                if let Some(addr) = part.strip_prefix("ip=") {
                    ip_addr = Some(addr.to_string());
                } else if let Some(mask) = part.strip_prefix("cidr_netmask=") {
                    netmask = mask.parse::<u8>().ok();
                }
            }

            if let (Some(address), Some(netmask)) = (ip_addr, netmask) {
                let interface = "eth0".to_string();

                return Some(VipConfig {
                    address,
                    netmask,
                    interface,
                });
            }
        }
    }

    None
}

/// Parse mount point from drbd-reactor config file content
/// Example: start = ["var-lib-mongodb.mount", ...] -> "/var/lib/mongodb"
fn parse_mount_point_from_config(content: &str) -> Option<String> {
    // Find lines with .mount units
    for line in content.lines() {
        if line.contains(".mount") {
            // Extract mount unit name from TOML array
            for part in line.split('"') {
                if part.ends_with(".mount") {
                    // Convert systemd mount unit name to path
                    // Example: var-lib-mongodb.mount -> /var/lib/mongodb
                    let name = part.strip_suffix(".mount")?;
                    return Some(format!("/{}", name.replace('-', "/")));
                }
            }
        }
    }
    None
}

/// Helper to get active node for a profile
async fn get_active_node(profile_name: &str) -> Option<String> {
    let output = run_shell_command(
        &format!("drbd-reactorctl status {} 2>/dev/null", profile_name),
        &format!("Get status for {}", profile_name),
    )
    .await
    .ok()?;

    if output.success() && !output.stdout.is_empty() {
        output
            .stdout
            .lines()
            .find(|line| line.contains("Currently active"))
            .and_then(|line| {
                if line.contains("Currently active on this node") {
                    Some(gethostname::gethostname().to_string_lossy().to_string())
                } else if line.contains("Currently active on node") {
                    let start = line.find('\'')?;
                    let end = line.rfind('\'')?;
                    if start < end {
                        Some(line[start + 1..end].to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
    } else {
        None
    }
}

/// GET /api/v1/ha/profiles
/// List all HA profiles
pub async fn list_profiles(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<HaProfileListResponse>> {
    let mut profiles = state.db.get_all_ha_profiles()?;

    // Enrich profiles with VIP, mount_point, and active_node info
    for profile in &mut profiles {
        // Get active node
        profile.active_node = get_active_node(&profile.name).await;

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
    }

    Ok(Json(HaProfileListResponse { profiles }))
}

/// GET /api/v1/ha/profiles/:id
/// Get a specific HA profile
pub async fn get_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<HaProfile>> {
    let mut profile = state
        .db
        .get_ha_profile(&id)?
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id)))?;

    // Enrich profile with VIP and mount_point from config file if not in DB
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

    Ok(Json(profile))
}

/// POST /api/v1/ha/profiles
/// Create a new HA profile
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

    // Validate inputs
    validator::validate_resource_name(&req.resource_name)?;

    // Mount point validation only for Generic and NFS
    if matches!(req.ha_type, HaType::Generic | HaType::Nfs) {
        validator::validate_mount_point(&req.mount_point)?;
    }

    // Filesystem type validation
    validator::validate_fs_type(&req.fs_type)?;

    // Validate services only for Generic
    if req.ha_type == HaType::Generic {
        for service in &req.services {
            validator::validate_service_name(service)?;
        }
    }

    if let Some(vip) = &req.vip {
        validator::validate_ip_address(&vip.address)?;
        validator::validate_netmask(vip.netmask)?;
    }

    // Check for duplicate
    if state.db.ha_profile_name_exists(&req.name)? {
        return Err(AppError::AlreadyExists(format!(
            "HA profile with name {} already exists",
            req.name
        )));
    }

    // Generate profile ID
    let profile_id = uuid::Uuid::new_v4().to_string();

    // --- Step 1: Storage Setup (LVM & DRBD) ---
    // Determine backing device path (DRBD resource disk)

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

        // Retrieve the storage pool from the database
        let storage_pool = state
            .db
            .get_storage_pool(pool_id)?
            .ok_or_else(|| AppError::NotFound(format!("Storage pool '{}' not found", pool_id)))?;

        // Create LVM logical volumes on all nodes
        let mut node_lvm_paths: Vec<(Node, String)> = Vec::new(); // (Node, LVM_path)

        for node in &all_nodes {
            let current_vg_name = storage_pool.name.clone(); // Use the same VG name as the pool
            let lv_name = format!("drbd-ha-lv-{}", req.resource_name); // Consistent LV name

            let lvm_provider = if node.is_local {
                LvmProvider::new_local(current_vg_name.clone())
            } else {
                // Get credentials for remote node (dummy)
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

            // Create the LVM logical volume on this node
            // Note: We don't check DB existence because each node needs its own LV
            let device_path = lvm_provider
                .create_volume(&lv_name, *volume_size_gb)
                .await
                .or_else(|e| {
                    // If creation fails, check if it already exists
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

            // Create Volume entry in DB if not exists (only once, not per node)
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

        // Build NodeConfigs for DRBD resource
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

        // Generate DRBD Resource Config
        let config_gen = ConfigGenerator::new()?;
        let resource_config = ResourceConfig {
            name: req.resource_name.clone(),
            port: drbd_port,
            minor: drbd_minor,
            device: format!("/dev/drbd{}", drbd_minor),
            nodes: node_configs,
            auto_promote: false, // HA managed resources should not auto-promote
            ..Default::default()
        };

        let config_content = config_gen.generate_drbd_resource(&resource_config)?;
        let config_path = ConfigPaths::drbd_resource_path(&req.resource_name);

        // Write locally
        tokio::fs::write(&config_path, &config_content)
            .await
            .map_err(|e| AppError::Config(format!("Failed to write DRBD config: {}", e)))?;

        tracing::info!("DRBD config written locally to {}", config_path);

        // Sync to remote nodes
        let remote_nodes: Vec<_> = all_nodes.iter().filter(|n| !n.is_local).collect();
        tracing::info!(
            "Found {} remote nodes for DRBD config sync",
            remote_nodes.len()
        );

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

        // Initialize Resource (create-md, up) on ALL nodes
        // Use --force to skip confirmation without relying on interactive stdin
        let create_md_cmd = format!("drbdadm create-md --force {}", req.resource_name);
        let up_cmd = format!("drbdadm up {}", req.resource_name);

        tracing::info!(
            "DRBD initialization: {} total nodes, {} remote nodes",
            all_nodes.len(),
            remote_nodes.len()
        );

        // Local init
        tracing::info!("Initializing DRBD on local node");
        run_shell_command(&create_md_cmd, "Create local metadata").await?;
        run_shell_command(&up_cmd, "Up local resource").await?;

        // Remote init
        tracing::info!(
            "Starting remote DRBD initialization for {} nodes",
            remote_nodes.len()
        );
        for node in &remote_nodes {
            tracing::info!(
                "Initializing DRBD on remote node: {} ({})",
                node.hostname,
                node.ip
            );
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

        // For DRBD resource config, we just need one of the LVM paths as they should be consistent
    } else {
        // NOTE: If not LVM, we assume DRBD resource is already set up
    }

    // Initialize generated units info
    let mut generated_units = GeneratedUnits::default();
    let mut messages = Vec::new();
    let extra_sync_files: Vec<(String, String)> = Vec::new(); // path, content

    // --- Step 2: HA Service Setup (Type Dependent) ---

    // Common: Mount Unit (Generic & NFS)
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
        tracing::info!("Generating mount unit for profile {}", req.name);
        let mount_info =
            MountUnitGenerator::generate(&req.resource_name, &req.mount_point, &req.fs_type)
                .await?;

        generated_units.mount_unit = Some(mount_info.unit_name.clone());
        generated_units.mount_unit_path = Some(mount_info.unit_path.clone());
        generated_units.drbd_device = Some(mount_info.device_path.clone());

        // Reload systemd and ensure unit is disabled/stopped
        // It must NOT be enabled as it is managed by drbd-reactor
        let _ = run_shell_command("systemctl daemon-reload", "Reload systemd after mount unit generation").await;
        let systemd = SystemdController::new().await?;
        let _ = systemd.disable_and_stop(&mount_info.unit_name).await;

        messages.push(format!("Generated mount unit: {}", mount_info.unit_name));
    }

    // Determine services to start based on type
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

            // Initialize Storage if we created it
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

                // 1. Promote & Mkfs
                let promote_out = run_shell_command(
                    &format!("drbdadm primary {}", req.resource_name),
                    "Promote for setup",
                )
                .await?;
                if !promote_out.success() {
                    return Err(AppError::Drbd(format!(
                        "Failed to promote for setup: {}",
                        promote_out.stderr
                    )));
                }

                let mkfs_cmd = format!(
                    "mkfs.{} /dev/drbd{}",
                    req.fs_type,
                    req.drbd_minor.unwrap_or(0)
                );
                let mkfs_out = run_shell_command(&mkfs_cmd, "Create filesystem").await?;
                if !mkfs_out.success() {
                    let _ = run_shell_command(
                        &format!("drbdadm secondary {}", req.resource_name),
                        "Cleanup secondary",
                    )
                    .await;
                    return Err(AppError::Internal(format!(
                        "Failed to create filesystem: {}",
                        mkfs_out.stderr
                    )));
                }

                // 2. Mount
                let mkdir_out = run_shell_command(
                    &format!("mkdir -p {}", req.mount_point),
                    "Create mount point",
                )
                .await?;
                if !mkdir_out.success() {
                    let _ = run_shell_command(
                        &format!("drbdadm secondary {}", req.resource_name),
                        "Cleanup secondary",
                    )
                    .await;
                    return Err(AppError::Internal(format!(
                        "Failed to create mount point: {}",
                        mkdir_out.stderr
                    )));
                }

                let mount_out = run_shell_command(
                    &format!(
                        "mount /dev/drbd{} {}",
                        req.drbd_minor.unwrap_or(0),
                        req.mount_point
                    ),
                    "Mount for setup",
                )
                .await?;
                if !mount_out.success() {
                    let _ = run_shell_command(
                        &format!("drbdadm secondary {}", req.resource_name),
                        "Cleanup secondary",
                    )
                    .await;
                    return Err(AppError::Internal(format!(
                        "Failed to mount for setup: {}",
                        mount_out.stderr
                    )));
                }

                // 3. Service Initialization (The "Silver Bullet")
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
                                tracing::error!(
                                    "Service initialization failed for {}: {}",
                                    service,
                                    e
                                );
                                // Cleanup before error
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

                // 4. Unmount & Demote
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

            // 1. Add services to promoter list
            // Use OCF agent for exports to manage them dynamically via drbd-reactor
            let fsid = NfsGenerator::generate_fsid(&req.resource_name);
            let ocf_resource = NfsGenerator::generate_ocf_exportfs(
                &req.resource_name,
                &req.mount_point,
                nfs_config,
                fsid,
            );
            
            // We do NOT manage nfs-server.service here directly as it should run globally.
            // The exportfs agent handles the export.
            let services = vec![ocf_resource];

            // 4. Initialize NFS State Storage (Critical for HA)
            // We need to temporarily mount the device to set up the state directory
            if req.lvm_pool_id.is_some() {
                // Only if we manage the storage
                state.send_progress(
                    &operation_id,
                    "create_ha_profile",
                    Some(&req.name),
                    35,
                    "Initializing NFS state storage...",
                    false,
                    None,
                );

                // Ensure Primary
                let promote_out = run_shell_command(
                    &format!("drbdadm primary {}", req.resource_name),
                    "Promote for NFS setup",
                )
                .await?;
                if !promote_out.success() {
                    return Err(AppError::Drbd(format!(
                        "Failed to promote for NFS setup: {}",
                        promote_out.stderr
                    )));
                }

                // Create FS if not exists (should have been done in Step 1 if lvm_pool_id set, but let's be safe)
                let mkfs_cmd = format!(
                    "mkfs.{} /dev/drbd{}",
                    req.fs_type,
                    req.drbd_minor.unwrap_or(0)
                );
                let mkfs_out = run_shell_command(&mkfs_cmd, "Create filesystem for NFS state").await?;
                if !mkfs_out.success() {
                    let _ = run_shell_command(
                        &format!("drbdadm secondary {}", req.resource_name),
                        "Cleanup secondary",
                    )
                    .await;
                    return Err(AppError::Internal(format!(
                        "Failed to create filesystem for NFS: {}",
                        mkfs_out.stderr
                    )));
                }

                // Mount
                let mkdir_out = run_shell_command(
                    &format!("mkdir -p {}", req.mount_point),
                    "Create mount point",
                )
                .await?;
                if !mkdir_out.success() {
                    let _ = run_shell_command(
                        &format!("drbdadm secondary {}", req.resource_name),
                        "Cleanup secondary",
                    )
                    .await;
                    return Err(AppError::Internal(format!(
                        "Failed to create mount point: {}",
                        mkdir_out.stderr
                    )));
                }

                let mount_out = run_shell_command(
                    &format!(
                        "mount /dev/drbd{} {}",
                        req.drbd_minor.unwrap_or(0),
                        req.mount_point
                    ),
                    "Mount for NFS setup",
                )
                .await?;
                if !mount_out.success() {
                    let _ = run_shell_command(
                        &format!("drbdadm secondary {}", req.resource_name),
                        "Cleanup secondary",
                    )
                    .await;
                    return Err(AppError::Internal(format!(
                        "Failed to mount for NFS setup: {}",
                        mount_out.stderr
                    )));
                }

                // Setup State Dir
                if let Err(e) = NfsGenerator::setup_nfs_state(&req.mount_point).await {
                    tracing::error!("Failed to setup local NFS state: {}", e);
                    // Try to cleanup
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

                // Unmount and Demote
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

                // 5. Setup Remote Nodes (Symlinks only)
                // We need to run a simplified setup on remotes: stop nfs, backup /var/lib/nfs, ln -s ...
                let remote_nodes: Vec<_> = all_nodes.iter().filter(|n| !n.is_local).collect();
                for node in &remote_nodes {
                    let credential = crate::core::SshCredential::Password("ignored".to_string());
                    // Simplified remote script
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

            // 6. No longer generating /etc/exports - managed by OCF agent

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

                // Execute on ALL nodes
                let creds = state.credentials.read().await;
                for node in &all_nodes {
                    let cmd_str = real_setup_cmds.join(" && ");
                    if node.is_local {
                        run_shell_command(&cmd_str, "Setup iSCSI Target locally").await?;
                    } else {
                        let credential = get_node_credential(&state, node).await?.unwrap(); // Should exist
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
                
                // Generate Setup Commands (for ExecStart)
                let setup_cmds = NvmeOfGenerator::generate_setup_commands(
                    &req.resource_name,
                    &drbd_dev,
                    nvmeof_config,
                    &vip.address,
                );

                // Generate Teardown Commands (for ExecStop)
                let teardown_cmds = NvmeOfGenerator::generate_teardown_commands(nvmeof_config);

                // Create a systemd unit content
                // We use nvmetcli shell commands wrapped in a script or inline
                // Since commands are a list, we can join them.
                // Note: nvmetcli might need to be run as `nvmetcli <<EOF ...` or just invoked if they are shell commands?
                // `NvmeOfGenerator` returns `nvmetcli` shell commands (e.g. `create subsystem ...`).
                // Actually `NvmeOfGenerator` commands are meant for `nvmetcli` shell? 
                // Let's check `NvmeOfGenerator`. It generates strings like "create subsystem ...".
                // These are NOT bash commands. They are `nvmetcli` commands.
                // So we need to pipe them to `nvmetcli`.
                
                let setup_script = setup_cmds.join("\n");
                let teardown_script = teardown_cmds.join("\n");

                let service_name = format!("drbd-ha-nvmeof-{}.service", req.resource_name);
                let service_content = format!(
r#"[Unit]
Description=NVMe-oF Target for {}
After=network.target drbd-reactor.service

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/bin/sh -c "echo '{}' | nvmetcli"
ExecStop=/bin/sh -c "echo '{}' | nvmetcli"

[Install]
WantedBy=multi-user.target
"#,
                    req.resource_name,
                    setup_script,
                    teardown_script
                );

                // Write service file to all nodes
                let creds = state.credentials.read().await;
                for node in &all_nodes {
                    let service_path = format!("/etc/systemd/system/{}", service_name);
                    
                    if node.is_local {
                        tokio::fs::write(&service_path, &service_content)
                            .await
                            .map_err(|e| AppError::Config(format!("Failed to write NVMe-oF service: {}", e)))?;
                        run_shell_command("systemctl daemon-reload", "Reload systemd").await?;
                    } else {
                        let credential = get_node_credential(&state, node).await?.unwrap();
                        state
                            .ssh_manager
                            .write_file(
                                &node.ip,
                                node.ssh_port,
                                &node.ssh_user,
                                &credential,
                                &service_path,
                                &service_content,
                            )
                            .await?;
                        state
                            .ssh_manager
                            .execute(
                                &node.ip,
                                node.ssh_port,
                                &node.ssh_user,
                                &credential,
                                "systemctl daemon-reload",
                            )
                            .await?;
                    }
                }
                drop(creds);

                messages.push("Configured NVMe-oF Target service on all nodes".to_string());
                
                // Return the service name to be started by drbd-reactor
                vec![service_name]
            } else {
                return Err(AppError::Validation(
                    "NVMe-oF configuration or VIP missing".to_string(),
                ));
            }
        }
    };

    // --- Step 3: Data Migration (Only for Generic/NFS with mount) ---
    let mut migration_result = None;
    if matches!(req.ha_type, HaType::Generic | HaType::Nfs) {
        if let Some(ref migration_opts) = req.migration {
            if migration_opts.migrate_data {
                state.send_progress(
                    &operation_id,
                    "create_ha_profile",
                    Some(&req.name),
                    40,
                    "Migrating data...",
                    false,
                    None,
                );
                tracing::info!("Starting data migration for profile {}", req.name);

                let source_path = migration_opts
                    .source_path
                    .clone()
                    .unwrap_or_else(|| req.mount_point.clone());

                let migration_config = MigrationConfig {
                    resource_name: req.resource_name.clone(),
                    source_path: source_path.clone(),
                    mount_point: req.mount_point.clone(),
                    fs_type: req.fs_type.clone(),
                    format_device: migration_opts.format_device,
                    services_to_stop: req.services.clone(),
                    preserve_permissions: migration_opts.preserve_permissions,
                };

                let result = DataMigration::migrate(migration_config, None).await?;

                migration_result = Some(MigrationResultInfo {
                    bytes_transferred: result.bytes_transferred,
                    source_path: result.source_path,
                    services_restarted: result.services_restarted,
                });

                messages.push(format!(
                    "Migrated {} bytes of data",
                    result.bytes_transferred
                ));
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

    // --- Step 4: Create HA Profile Object ---
    let mut profile = HaProfile {
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
            dependencies_as: req.dependencies_as.clone(),
            target_as: req.target_as.clone(),
            on_quorum_loss: req.on_quorum_loss.clone(),
            preferred_nodes: req.preferred_nodes.clone(),
            preferred_nodes_policy: req.preferred_nodes_policy.clone(),
            sleep_before_promote_factor: req.sleep_before_promote_factor,
        },
        status: HaProfileStatus::Unknown,
        active_node: None,
        generated_units: generated_units.clone(),
        nfs: req.nfs.clone(),
        iscsi: req.iscsi.clone(),
        nvmeof: req.nvmeof.clone(),
        generated_config: None, // Will be set after generation
    };

    // --- Step 5: Generate Promoter Config ---
    let config_gen = ConfigGenerator::new()?;

    // We need to override the profile's services list for the generator
    let mut profile_for_gen = profile.clone();
    profile_for_gen.promoter.services = _start_services;

    let promoter_config = ConfigGenerator::promoter_from_profile(&profile_for_gen);
    let config_content = config_gen.generate_promoter(&promoter_config)?;

    // Update profile with generated config
    profile.generated_config = Some(config_content.clone());

    let config_path = ConfigPaths::promoter_path(&req.name);

    // Ensure config directory exists
    let config_dir = std::path::Path::new(&config_path).parent().unwrap();
    if !config_dir.exists() {
        tokio::fs::create_dir_all(config_dir)
            .await
            .map_err(|e| AppError::Config(format!("Failed to create config dir: {}", e)))?;
    }

    // Write promoter configuration
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

    // --- Step 6: Disable Managed Services (Auto-disable) ---
    // Only for Generic (user provided services) and NFS (nfs-server)
    let services_to_disable = match req.ha_type {
        HaType::Generic => req.services.clone(),
        HaType::Nfs => vec!["nfs-server.service".to_string()],
        _ => vec![],
    };

    let mut disabled_services = Vec::new();
    if req.auto_disable_services && !services_to_disable.is_empty() {
        // Disable locally
        let systemd = SystemdController::new().await?;
        for service in &services_to_disable {
            if systemd.is_enabled(service).await.unwrap_or(false) {
                if let Ok(()) = systemd.disable_and_stop(service).await {
                    tracing::info!("Disabled service {} for HA profile {}", service, req.name);
                    disabled_services.push(service.clone());
                } else {
                    tracing::warn!(
                        "Failed to disable service {} for HA profile {}",
                        service,
                        req.name
                    );
                }
            }
        }

        // Disable on remote nodes
        let remote_nodes: Vec<_> = all_nodes.iter().filter(|n| !n.is_local).collect();

        for node in &remote_nodes {
            let credential = crate::core::SshCredential::Password("ignored".to_string());

            for service in &services_to_disable {
                let disable_cmd =
                    format!("systemctl disable --now {} 2>/dev/null || true", service);
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
                    tracing::warn!("Failed to disable {} on {}: {}", service, node.hostname, e);
                } else {
                    tracing::info!("Disabled {} on {}", service, node.hostname);
                }
            }
        }

        if !disabled_services.is_empty() {
            messages.push(format!(
                "Disabled {} service(s) on all nodes",
                disabled_services.len()
            ));
        }
    }

    // Step 7: Reload systemd daemon to recognize new unit files and restart reactor
    let systemd_reload = run_shell_command(
        "systemctl daemon-reload && systemctl restart drbd-reactor",
        "Reload systemd and restart reactor",
    )
    .await;
    if systemd_reload.is_err() || !systemd_reload.unwrap().success() {
        tracing::warn!("Failed to reload systemd/restart reactor");
    }

    state.send_progress(
        &operation_id,
        "create_ha_profile",
        Some(&req.name),
        80,
        "Syncing configuration to cluster nodes...",
        false,
        None,
    );

    // Step 8: Sync configuration to all cluster nodes
    let mut synced_nodes = Vec::new();
    {
        // Read mount unit content
        let mount_unit_content = if let Some(ref path) = generated_units.mount_unit_path {
            tokio::fs::read_to_string(path).await.ok()
        } else {
            None
        };

        // Read service override contents
        let mut service_override_contents = Vec::new();
        for so in &generated_units.service_overrides {
            if let Ok(content) = tokio::fs::read_to_string(&so.override_path).await {
                service_override_contents.push((so.override_path.clone(), content));
            }
        }

        // Add extra files (NFS exports, etc.)
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

        // Build sync config
        let sync_config = HaSyncConfig {
            mount_unit: generated_units
                .mount_unit_path
                .clone()
                .zip(mount_unit_content),
            service_overrides: service_override_contents,
            promoter_config: (config_path.clone(), config_content.clone()),
        };

        // Sync to remote nodes
        let cluster_sync = ClusterSync::new(
            state.ssh_manager.clone(),
            state.db.clone(),
            state.credentials.clone(),
        );

        match cluster_sync.sync_ha_config(&sync_config).await {
            Ok(nodes) => {
                if !nodes.is_empty() {
                    messages.push(format!("Synced to {} node(s)", nodes.len()));
                    synced_nodes = nodes;
                }
            }
            Err(e) => {
                tracing::warn!("Failed to sync to some nodes: {}", e);
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

    // Step 9: Store profile to database
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
        }),
    ))
}

/// Query parameters for delete profile
#[derive(Debug, Deserialize)]
pub struct DeleteProfileQuery {
    /// Also delete the associated DRBD resource
    #[serde(default)]
    pub delete_resource: bool,
    /// Also delete the promoter configuration file from disk
    /// WARNING: This will remove the .toml file from /etc/drbd-reactor.d/
    /// The profile can still be re-imported from the file if it exists
    #[serde(default)]
    pub delete_config_file: bool,
}

/// DELETE /api/v1/ha/profiles/:id
/// Delete an HA profile (accepts ID or name)
pub async fn delete_profile(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
    Query(_query): Query<DeleteProfileQuery>,
) -> AppResult<StatusCode> {
    // Try by ID first, then by name
    let profile = state
        .db
        .get_ha_profile(&id_or_name)?
        .or(state.db.get_ha_profile_by_name(&id_or_name)?)
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    let resource_name = profile.resource_name.clone();

    // Step 0: Deactivate if active on this node (stop services, unmount, demote)
    // Check if DRBD is Primary on local node
    let status_cmd = format!("drbdadm status {} 2>/dev/null", profile.resource_name);
    let is_primary = match run_shell_command(
        &status_cmd,
        &format!("Check DRBD status for {}", profile.resource_name),
    )
    .await
    {
        Ok(output) => output.stdout.contains("role:Primary"),
        Err(_) => false,
    };

    if is_primary {
        tracing::info!(
            "Profile {} is active on this node, deactivating before delete...",
            profile.name
        );

        // Remove VIP if configured
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

        // Stop services in reverse order
        let systemd = SystemdController::new().await?;
        for service in profile.promoter.services.iter().rev() {
            if let Err(e) = systemd.stop(service).await {
                tracing::warn!("Failed to stop service {}: {}", service, e);
            }
        }

        // Wait for services to release file handles
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Kill any remaining processes using the mount point (except our own process)
        let my_pid = std::process::id();
        let kill_cmd = format!(
            "for pid in $(lsof -t {} 2>/dev/null | grep -v {}); do kill -9 $pid 2>/dev/null || true; done",
            profile.mount_point, my_pid
        );
        let _ = run_shell_command(
            &kill_cmd,
            &format!("Kill processes using mount point {}", profile.mount_point),
        )
        .await;

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Unmount the DRBD device
        let umount_cmd = format!(
            "umount {} 2>/dev/null || umount -l {} 2>/dev/null || true",
            profile.mount_point, profile.mount_point
        );
        let _ = run_shell_command(&umount_cmd, &format!("Unmount {}", profile.mount_point)).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Demote DRBD resource
        let demote_cmd = format!("drbdadm secondary {}", profile.resource_name);
        let demote_output = run_shell_command(
            &demote_cmd,
            &format!("Demote DRBD resource {}", profile.resource_name),
        )
        .await;
        if let Ok(output) = demote_output {
            if !output.success() {
                tracing::warn!(
                    "Failed to demote DRBD resource {}: {}",
                    profile.resource_name,
                    output.stderr
                );
            }
        }
    }

    // Step 1: Remove service overrides
    let service_names: Vec<String> = profile.promoter.services.clone();
    if let Err(e) = ServiceOverrideGenerator::remove_for_services(&service_names).await {
        tracing::warn!("Failed to remove service overrides: {}", e);
    }

    // Step 2: Remove mount unit
    if let Err(e) = MountUnitGenerator::remove(&profile.mount_point).await {
        tracing::warn!("Failed to remove mount unit: {}", e);
    }

    // Step 3: Delete promoter configuration file

    // We always delete the config file when deleting the profile to prevent "zombie" configurations

    let config_path = ConfigPaths::promoter_path(&profile.name);

    if tokio::fs::metadata(&config_path).await.is_ok() {
        match tokio::fs::remove_file(&config_path).await {
            Ok(_) => {
                tracing::info!("Deleted promoter config file: {}", config_path);
            }

            Err(e) => {
                tracing::warn!("Failed to delete config file {}: {}", config_path, e);
            }
        }
    }

    // Step 4: Reload systemd daemon
    let _ = run_shell_command("systemctl daemon-reload", "Reload systemd daemon").await;

    // Step 5: Remove configuration from remote nodes
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
    if let Err(e) = cluster_sync.remove_ha_config(&sync_config).await {
        tracing::warn!("Failed to remove config from remote nodes: {}", e);
    }

    // Step 6: Remove from database
    state.db.delete_ha_profile(&profile.id)?;

    tracing::info!("Deleted HA profile: {} ({})", profile.name, profile.id);

    // Step 7: Delete the DRBD resource configuration
    // We force deletion to ensure clean state as requested
    {
        use crate::core::drbd_cmd::DrbdCmd;

        tracing::info!(
            "Bringing down and deleting DRBD resource: {}",
            resource_name
        );

        // Bring down the resource on local node
        if let Ok(down_cmd) = DrbdCmd::down_cmd(&resource_name) {
            let _ = run_shell_command(
                &down_cmd,
                &format!("Bring down DRBD resource {}", resource_name),
            )
            .await;
        }

        // Delete DRBD config on local node
        let drbd_config_path = ConfigPaths::drbd_resource_path(&resource_name);
        tracing::info!("Deleting local DRBD config: {}", drbd_config_path);
        if tokio::fs::metadata(&drbd_config_path).await.is_ok() {
            if let Err(e) = tokio::fs::remove_file(&drbd_config_path).await {
                tracing::warn!(
                    "Failed to delete local DRBD config {}: {}",
                    drbd_config_path,
                    e
                );
            } else {
                tracing::info!("Deleted local DRBD config: {}", drbd_config_path);
            }
        } else {
            tracing::warn!("Local DRBD config not found at {}", drbd_config_path);
        }

        // Delete DRBD config on remote nodes
        if let Err(e) = cluster_sync.remove_drbd_resource(&resource_name).await {
            tracing::warn!("Failed to remove DRBD resource from remote nodes: {}", e);
        }
    }

    // Step 8: Delete the LVM logical volume (Force cleanup)
    // We attempt to find the volume in the DB. If found, we delete it from all nodes.
    if let Some(volume) = state.db.get_volume_by_drbd_res(&resource_name)? {
        tracing::info!(
            "Deleting associated LVM logical volume '{}' (id: {})",
            volume.name,
            volume.id
        );
        if let Ok(Some(storage_pool)) = state.db.get_storage_pool(&volume.pool_id) {
            // Iterate over all nodes to delete the LVM logical volume
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
                    tracing::warn!("Skipping LVM delete on {}: no credential", node.hostname);
                    continue;
                };

                // Force remove try
                if let Err(e) = lvm_provider.delete_volume(&volume.name).await {
                    tracing::warn!(
                        "Failed to delete LVM volume '{}' on node '{}': {}",
                        volume.name,
                        node.hostname,
                        e
                    );
                } else {
                    tracing::info!(
                        "Deleted LVM volume '{}' on node '{}'",
                        volume.name,
                        node.hostname
                    );
                }
            }

            // Update pool's free size (best effort)
            if let Ok(Some(vg_info)) = crate::core::lvm_utils::get_vg_info(&storage_pool.name).await
            {
                let _ = state.db.update_storage_pool_sizes(
                    &storage_pool.id,
                    vg_info.size,
                    vg_info.free,
                );
            }
        } else {
            tracing::warn!(
                "Storage pool for volume {} not found in DB, cannot delete LV",
                volume.name
            );
        }

        // Delete volume record from database
        state.db.delete_volume(&volume.id)?;
        tracing::info!("Deleted LVM volume record from database: {}", volume.id);
    } else {
        tracing::warn!(
            "No volume record found for resource {}, skipping LVM deletion",
            resource_name
        );
    }

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/ha/profiles/:id/status
/// Get detailed status of an HA profile (accepts ID or name)
pub async fn get_profile_status(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
) -> AppResult<Json<HaProfileStatusResponse>> {
    let profile = state
        .db
        .get_ha_profile(&id_or_name)?
        .or(state.db.get_ha_profile_by_name(&id_or_name)?)
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    // Get drbd-reactorctl status to find active node
    let (active_node, reactor_status_raw) = {
        let cmd = format!("drbd-reactorctl status {} 2>/dev/null", profile.name);
        let output = run_shell_command(
            &cmd,
            &format!("Get drbd-reactorctl status for {}", profile.name),
        )
        .await?;
        tracing::info!(
            "drbd-reactorctl command success: {}, stdout len: {}, stderr: {}",
            output.success(),
            output.stdout.len(),
            output.stderr
        );
        if output.success() && !output.stdout.is_empty() {
            tracing::info!("Reactor status output: {}", output.stdout);
            // Parse "Currently active on node 'xxx'" or "Currently active on this node" from output
            let active = output
                .stdout
                .lines()
                .find(|line| line.contains("Currently active"))
                .and_then(|line| {
                    if line.contains("Currently active on this node") {
                        // It's active on the local node, get hostname
                        Some(gethostname::gethostname().to_string_lossy().to_string())
                    } else if line.contains("Currently active on node") {
                        // Extract node name between single quotes, e.g. Currently active on node 'gui02'
                        let start = line.find('\'')?;
                        let end = line.rfind('\'')?;
                        if start < end {
                            Some(line[start + 1..end].to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });
            (active, Some(output.stdout))
        } else {
            (None, None)
        }
    };

    // Check DRBD resource status using drbdadm status (human-readable format)
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

    // Extract role for status determination
    let drbd_role = drbd.as_ref().map(|d| d.role.as_str());

    // Check service statuses from drbd-reactorctl output (parse tree structure)
    let mut service_statuses = Vec::new();
    if let Some(raw) = &reactor_status_raw {
        tracing::info!("Parsing reactor status for services");
        // Parse drbd-reactorctl status output to get service states
        for line in raw.lines() {
            let trimmed = line.trim();
            // Skip empty lines and header lines
            if trimmed.is_empty() || trimmed.contains("Promoter:") || trimmed.ends_with(".toml:") {
                continue;
            }

            // Check if line starts with status symbol
            let is_active = trimmed.starts_with('○') || trimmed.starts_with("○");
            let is_failed = trimmed.starts_with('×') || trimmed.starts_with("×");

            if !is_active && !is_failed {
                continue;
            }

            // Remove leading symbol and tree characters to get service name
            // The format is: "○ drbd-services@xxx" or "× ├─ service.name"
            let without_symbol = if is_active {
                trimmed.strip_prefix('○').or(trimmed.strip_prefix("○"))
            } else {
                trimmed.strip_prefix('×').or(trimmed.strip_prefix("×"))
            }
            .unwrap_or(trimmed);

            // Remove leading symbol and tree characters to get service name
            let mut service_line = without_symbol.trim().to_string(); // Work with owned string, trimmed

            // Strip tree prefixes
            if let Some(s) = service_line.strip_prefix("├─") {
                service_line = s.to_string();
            } else if let Some(s) = service_line.strip_prefix("└─") {
                service_line = s.to_string();
            } else if let Some(s) = service_line.strip_prefix("─") {
                service_line = s.to_string();
            }

            // Get the first word after stripping prefixes, which should be the service name
            let name = service_line.split_whitespace().next().unwrap_or("");

            // Skip internal drbd services and empty names
            if name.is_empty()
                || name.starts_with("drbd-services@")
                || name.starts_with("drbd-promote@")
            {
                continue;
            }

            tracing::info!("Adding service: {} (active: {})", name, is_active);

            // Unescape systemd service name (replace \x2d with -)
            let clean_name = name.replace("\\x2d", "-");

            service_statuses.push(ServiceStatusInfo {
                name: clean_name,
                active: is_active,
                state: if is_active {
                    "active".to_string()
                } else {
                    "failed".to_string()
                },
                enabled: false,
                active_since: None,
            });
        }
        tracing::info!("Total services parsed: {}", service_statuses.len());

        // Query systemd for enabled status of each service
        let systemd = SystemdController::new().await?;
        for service_info in &mut service_statuses {
            // Skip OCF services (not systemd units)
            if service_info.name.starts_with("ocf.rs@") {
                continue;
            }

            // Query systemd status for this service
            if let Ok(status) = systemd.status(&service_info.name).await {
                service_info.enabled = status.is_enabled();
            }
        }
    }

    // Detect mount point from mount unit name (e.g., var-lib-mongodb.mount -> /var/lib/mongodb)
    let mount_point = service_statuses
        .iter()
        .find(|s| s.name.ends_with(".mount"))
        .and_then(|s| {
            let name = s.name.strip_suffix(".mount")?;
            // Convert systemd unit name to path: replace - with /
            Some(format!("/{}", name.replace('-', "/")))
        });

    // Check configuration visibility
    let promoter_config_path = ConfigPaths::promoter_path(&profile.name);
    let promoter_config_exists = tokio::fs::metadata(&promoter_config_path).await.is_ok();
    let systemd = SystemdController::new().await?;
    let reactor_service_status = systemd.status("drbd-reactor.service").await?;
    let config = ConfigVisibility {
        promoter_config_exists,
        promoter_config_path: promoter_config_path.clone(),
        reactor_running: reactor_service_status.is_running(),
    };

    // Check VIP status: if config contains IPaddr2 and profile has VIP configured, mark active when active_node exists
    // Also check if there's any ocf.rs VIP service in the parsed services
    let vip_active = if let Some(vip) = &profile.vip {
        let mut active = false;

        // First check if VIP OCF resource is in the promoter config
        if promoter_config_exists {
            if let Ok(cfg) = tokio::fs::read_to_string(&promoter_config_path).await {
                if cfg.contains("ocf:heartbeat:IPaddr2") && active_node.is_some() {
                    active = true;
                }
            }
        }

        // If not determined yet and we have an active node, assume VIP is active
        if !active && active_node.is_some() {
            active = true;
        }

        // Fallback to local interface check
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
        // No VIP in profile, but check if there's a VIP service in the parsed services
        let has_vip_service = service_statuses
            .iter()
            .any(|s| s.name.contains("vip") && s.name.contains("ocf.rs@"));

        if has_vip_service {
            let vip_service_active = service_statuses
                .iter()
                .find(|s| s.name.contains("vip") && s.name.contains("ocf.rs@"))
                .map(|s| s.active)
                .unwrap_or(false);
            Some(vip_service_active)
        } else {
            None
        }
    };

    // Determine overall status based on active_node
    let status = if active_node.is_some() {
        // If drbd-reactorctl shows active on a node, it's Active
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

    Ok(Json(HaProfileStatusResponse {
        id: profile.id,
        name: profile.name,
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

/// POST /api/v1/ha/profiles/:id/activate
/// Activate an HA profile (promote DRBD and start services)
pub async fn activate_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<HaProfileStatusResponse>> {
    tracing::info!(
        "activate_profile: Starting activation for profile id={}",
        id
    );

    let profile = state
        .db
        .get_ha_profile(&id)?
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id)))?;
    tracing::info!(
        "activate_profile: Found profile '{}', resource='{}', mount='{}'",
        profile.name,
        profile.resource_name,
        profile.mount_point
    );

    let operation_id = uuid::Uuid::new_v4().to_string();
    state.send_progress(
        &operation_id,
        "activate_profile",
        Some(&profile.name),
        0,
        "Checking DRBD resource...",
        false,
        None,
    );

    // First, ensure the DRBD resource is up (loaded in kernel)
    let up_cmd = format!("drbdadm up {}", profile.resource_name);
    tracing::info!("activate_profile: Running '{}'", up_cmd);
    let up_output = run_shell_command(
        &up_cmd,
        &format!("Bring up DRBD resource {}", profile.resource_name),
    )
    .await?;
    tracing::info!(
        "activate_profile: up result: success={}, stdout='{}', stderr='{}'",
        up_output.success(),
        up_output.stdout.trim(),
        up_output.stderr.trim()
    );

    if !up_output.success() {
        // Check if metadata is missing
        if up_output.stderr.contains("No valid meta data") {
            tracing::info!("activate_profile: No metadata found, creating...");
            state.send_progress(
                &operation_id,
                "activate_profile",
                Some(&profile.name),
                5,
                "Creating DRBD metadata...",
                false,
                None,
            );

            // Create metadata
            let create_md_cmd = format!("drbdadm create-md --force {}", profile.resource_name);
            tracing::info!("activate_profile: Running '{}'", create_md_cmd);
            let md_output = run_shell_command(
                &create_md_cmd,
                &format!("Create DRBD metadata for {}", profile.resource_name),
            )
            .await?;
            tracing::info!(
                "activate_profile: create-md result: success={}, stderr='{}'",
                md_output.success(),
                md_output.stderr.trim()
            );

            if !md_output.success() {
                state.send_progress(
                    &operation_id,
                    "activate_profile",
                    Some(&profile.name),
                    10,
                    &format!("Failed to create metadata: {}", md_output.stderr),
                    true,
                    Some(false),
                );
                return Err(AppError::Drbd(format!(
                    "Failed to create DRBD metadata: {}",
                    md_output.stderr
                )));
            }

            // Try up again
            tracing::info!("activate_profile: Retrying up command");
            let up_retry = run_shell_command(
                &up_cmd,
                &format!("Retry bringing up DRBD resource {}", profile.resource_name),
            )
            .await?;
            tracing::info!(
                "activate_profile: up retry result: success={}, stderr='{}'",
                up_retry.success(),
                up_retry.stderr.trim()
            );

            if !up_retry.success() && !up_retry.stderr.contains("already") {
                state.send_progress(
                    &operation_id,
                    "activate_profile",
                    Some(&profile.name),
                    10,
                    &format!("Failed to bring up resource: {}", up_retry.stderr),
                    true,
                    Some(false),
                );
                return Err(AppError::Drbd(format!(
                    "Failed to bring up DRBD resource: {}",
                    up_retry.stderr
                )));
            }
        } else if !up_output.stderr.contains("already") {
            tracing::warn!("activate_profile: drbdadm up failed: {}", up_output.stderr);
        }
    }

    // Check current status - if Diskless, try to attach
    let status_cmd = format!("drbdadm status {}", profile.resource_name);
    tracing::info!("activate_profile: Running '{}'", status_cmd);
    let status_output = run_shell_command(
        &status_cmd,
        &format!("Get DRBD status for {}", profile.resource_name),
    )
    .await?;
    tracing::info!("activate_profile: status output:\n{}", status_output.stdout);

    if status_output.stdout.contains("disk:Diskless") {
        tracing::info!("activate_profile: Disk is Diskless, trying to attach");
        state.send_progress(
            &operation_id,
            "activate_profile",
            Some(&profile.name),
            8,
            "Attaching disk...",
            false,
            None,
        );

        // Try to attach
        let attach_cmd = format!("drbdadm attach {}", profile.resource_name);
        tracing::info!("activate_profile: Running '{}'", attach_cmd);
        let attach_output = run_shell_command(
            &attach_cmd,
            &format!("Attach DRBD resource {}", profile.resource_name),
        )
        .await?;
        tracing::info!(
            "activate_profile: attach result: success={}, stderr='{}'",
            attach_output.success(),
            attach_output.stderr.trim()
        );

        if !attach_output.success() {
            // If attach fails due to missing metadata, create it
            if attach_output.stderr.contains("No valid meta data") {
                tracing::info!(
                    "activate_profile: Attach failed due to missing metadata, creating..."
                );
                let create_md_cmd =
                    format!("drbdadm create-md --force {} 2>&1", profile.resource_name);
                let md_result = run_shell_command(
                    &create_md_cmd,
                    &format!("Create DRBD metadata for {}", profile.resource_name),
                )
                .await;
                tracing::info!("activate_profile: create-md result: {:?}", md_result);

                // Retry attach
                let retry_result = run_shell_command(
                    &attach_cmd,
                    &format!("Retry attaching DRBD resource {}", profile.resource_name),
                )
                .await?;
                tracing::info!("activate_profile: attach retry result: {:?}", retry_result);
            }
        }
    }

    // If StandAlone, try to connect
    if status_output.stdout.contains("connection:StandAlone") {
        tracing::info!("activate_profile: Connection is StandAlone, trying to connect");
        state.send_progress(
            &operation_id,
            "activate_profile",
            Some(&profile.name),
            9,
            "Connecting to peers...",
            false,
            None,
        );
        let connect_cmd = format!("drbdadm connect {}", profile.resource_name);
        tracing::info!("activate_profile: Running '{}'", connect_cmd);
        let connect_result = run_shell_command(
            &connect_cmd,
            &format!("Connect DRBD resource {}", profile.resource_name),
        )
        .await?;
        tracing::info!("activate_profile: connect result: {:?}", connect_result);
    }

    state.send_progress(
        &operation_id,
        "activate_profile",
        Some(&profile.name),
        10,
        "Promoting DRBD resource...",
        false,
        None,
    );

    // Try to promote DRBD resource
    let promote_cmd = format!("drbdadm primary {}", profile.resource_name);
    tracing::info!("activate_profile: Running '{}'", promote_cmd);
    let output = run_shell_command(
        &promote_cmd,
        &format!("Promote DRBD resource {}", profile.resource_name),
    )
    .await?;
    tracing::info!(
        "activate_profile: primary result: success={}, stderr='{}'",
        output.success(),
        output.stderr.trim()
    );

    if !output.success() {
        // Check if this is a new resource (all nodes Inconsistent) - need --force
        if output.stderr.contains("Need access to UpToDate data") {
            tracing::info!("activate_profile: Need UpToDate data, trying force promote...");
            state.send_progress(
                &operation_id,
                "activate_profile",
                Some(&profile.name),
                10,
                "Resource not synced yet, skipping initial sync...",
                false,
                None,
            );

            // Skip initial sync on new resource
            let skip_sync_cmd = format!(
                "drbdadm new-current-uuid --clear-bitmap {}",
                profile.resource_name
            );
            tracing::info!("activate_profile: Running '{}'", skip_sync_cmd);
            let skip_output = run_shell_command(
                &skip_sync_cmd,
                &format!("Skip initial sync for {}", profile.resource_name),
            )
            .await?;
            tracing::info!(
                "activate_profile: skip sync result: success={}, stderr='{}'",
                skip_output.success(),
                skip_output.stderr.trim()
            );

            // Try force promote
            let force_promote_cmd = format!("drbdadm primary --force {}", profile.resource_name);
            tracing::info!("activate_profile: Running '{}'", force_promote_cmd);
            let force_output = run_shell_command(
                &force_promote_cmd,
                &format!("Force promote DRBD resource {}", profile.resource_name),
            )
            .await?;
            tracing::info!(
                "activate_profile: force primary result: success={}, stderr='{}'",
                force_output.success(),
                force_output.stderr.trim()
            );

            if !force_output.success() {
                state.send_progress(
                    &operation_id,
                    "activate_profile",
                    Some(&profile.name),
                    20,
                    &format!("Failed to force promote: {}", force_output.stderr),
                    true,
                    Some(false),
                );
                state.send_notification(
                    NotificationLevel::Error,
                    "Activation Failed",
                    &format!(
                        "Failed to promote '{}': {}",
                        profile.name, force_output.stderr
                    ),
                );
                return Err(AppError::Drbd(format!(
                    "Failed to promote resource: {}",
                    force_output.stderr
                )));
            }
            tracing::info!("activate_profile: Force promote succeeded");
        } else {
            tracing::error!("activate_profile: Promote failed: {}", output.stderr);
            state.send_progress(
                &operation_id,
                "activate_profile",
                Some(&profile.name),
                20,
                &format!("Failed to promote: {}", output.stderr),
                true,
                Some(false),
            );
            state.send_notification(
                NotificationLevel::Error,
                "Activation Failed",
                &format!("Failed to promote '{}': {}", profile.name, output.stderr),
            );
            return Err(AppError::Drbd(format!(
                "Failed to promote resource: {}",
                output.stderr
            )));
        }
    }

    tracing::info!("activate_profile: DRBD promoted to Primary successfully");

    // Check if filesystem exists, if not, format it
    let drbd_device = format!("/dev/drbd/by-res/{}/0", profile.resource_name);
    let check_fs_cmd = format!("blkid -o value -s TYPE {}", drbd_device);
    tracing::info!("activate_profile: Checking filesystem on {}", drbd_device);
    let fs_check = run_shell_command(&check_fs_cmd, "Check filesystem type").await?;

    if fs_check.stdout.trim().is_empty() {
        // No filesystem, need to format
        state.send_progress(
            &operation_id,
            "activate_profile",
            Some(&profile.name),
            25,
            &format!("Formatting {} filesystem...", profile.fs_type),
            false,
            None,
        );

        let mkfs_cmd = match profile.fs_type.as_str() {
            "xfs" => format!("mkfs.xfs -f {}", drbd_device),
            "ext4" => format!("mkfs.ext4 -F {}", drbd_device),
            _ => format!("mkfs.{} {}", profile.fs_type, drbd_device),
        };

        tracing::info!("activate_profile: Formatting with: {}", mkfs_cmd);
        let mkfs_output = run_shell_command(&mkfs_cmd, "Format filesystem").await?;

        if !mkfs_output.success() {
            return Err(AppError::Drbd(format!(
                "Failed to format filesystem: {}",
                mkfs_output.stderr
            )));
        }
        tracing::info!("activate_profile: Filesystem formatted successfully");
    } else {
        tracing::info!(
            "activate_profile: Existing filesystem found: {}",
            fs_check.stdout.trim()
        );
    }

    state.send_progress(
        &operation_id,
        "activate_profile",
        Some(&profile.name),
        30,
        "Mounting DRBD device...",
        false,
        None,
    );

    // Mount the DRBD device
    let mount_cmd = format!(
        "mkdir -p {} && mount {} {}",
        profile.mount_point, drbd_device, profile.mount_point
    );
    tracing::info!("activate_profile: Running '{}'", mount_cmd);
    let mount_output = run_shell_command(
        &mount_cmd,
        &format!(
            "Mount DRBD device {} to {}",
            profile.resource_name, profile.mount_point
        ),
    )
    .await?;
    tracing::info!(
        "activate_profile: mount result: success={}, stderr='{}'",
        mount_output.success(),
        mount_output.stderr.trim()
    );

    // Check if mount point is writable (basic permission check)
    let test_cmd = format!(
        "touch {}/.drbd-ha-test && rm {}/.drbd-ha-test",
        profile.mount_point, profile.mount_point
    );
    tracing::info!(
        "activate_profile: Testing write permission on {}",
        profile.mount_point
    );
    let test_output = run_shell_command(
        &test_cmd,
        &format!(
            "Test write permission on mount point {}",
            profile.mount_point
        ),
    )
    .await?;
    if !test_output.success() {
        tracing::warn!(
            "activate_profile: Mount point {} may have permission issues",
            profile.mount_point
        );
        state.send_notification(
            NotificationLevel::Warning,
            "Permission Warning",
            &format!("Mount point {} may have permission issues. Please check ownership (e.g., chown -R <user>:<group> {})", profile.mount_point, profile.mount_point)
        );
    } else {
        tracing::info!(
            "activate_profile: Mount point {} is writable",
            profile.mount_point
        );
    }

    // Setup NFS state directory if this is an NFS profile
    if profile.ha_type == HaType::Nfs {
        tracing::info!("activate_profile: Setting up NFS state directory");
        state.send_progress(
            &operation_id,
            "activate_profile",
            Some(&profile.name),
            40,
            "Setting up NFS state directory...",
            false,
            None,
        );

        use crate::core::NfsGenerator;
        if let Err(e) = NfsGenerator::setup_nfs_state(&profile.mount_point).await {
            tracing::error!("Failed to setup NFS state: {}", e);
            return Err(e);
        }
        tracing::info!("activate_profile: NFS state directory setup complete");

        // Refresh NFS exports
        tracing::info!("activate_profile: Refreshing NFS exports");
        crate::core::run_shell_command("exportfs -ra", "Refresh NFS exports").await?;
        tracing::info!("activate_profile: NFS exports refreshed");
    }

    state.send_progress(
        &operation_id,
        "activate_profile",
        Some(&profile.name),
        50,
        "Starting services...",
        false,
        None,
    );

    // Start services in order
    tracing::info!(
        "activate_profile: Starting {} services: {:?}",
        profile.promoter.services.len(),
        profile.promoter.services
    );
    let systemd = SystemdController::new().await?;
    for service in &profile.promoter.services {
        tracing::info!("activate_profile: Starting service '{}'", service);
        systemd.start(service).await?;
        tracing::info!("activate_profile: Service '{}' started", service);
    }

    // Add VIP if configured
    if let Some(vip) = &profile.vip {
        tracing::info!(
            "activate_profile: Configuring VIP {}/{} on {}",
            vip.address,
            vip.netmask,
            vip.interface
        );
        state.send_progress(
            &operation_id,
            "activate_profile",
            Some(&profile.name),
            80,
            "Configuring VIP...",
            false,
            None,
        );
        let vip_cmd = format!(
            "ip addr add {}/{} dev {}",
            vip.address, vip.netmask, vip.interface
        );
        let _ = run_shell_command(
            &vip_cmd,
            &format!(
                "Configure VIP {}/{} on {}",
                vip.address, vip.netmask, vip.interface
            ),
        )
        .await;
    }

    state.send_progress(
        &operation_id,
        "activate_profile",
        Some(&profile.name),
        100,
        "Profile activated successfully",
        true,
        Some(true),
    );
    state.send_notification(
        NotificationLevel::Success,
        "Profile Activated",
        &format!("HA profile '{}' is now active", profile.name),
    );

    // Get updated status
    get_profile_status(State(state), Path(id)).await
}

/// POST /api/v1/ha/profiles/:id/deactivate
/// Deactivate an HA profile (stop services and demote DRBD)
pub async fn deactivate_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<HaProfileStatusResponse>> {
    tracing::info!(
        "deactivate_profile: Starting deactivation for profile id={}",
        id
    );

    let profile = state
        .db
        .get_ha_profile(&id)?
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id)))?;
    tracing::info!(
        "deactivate_profile: Found profile '{}', resource='{}', mount='{}'",
        profile.name,
        profile.resource_name,
        profile.mount_point
    );

    let operation_id = uuid::Uuid::new_v4().to_string();

    // Remove VIP if configured
    if let Some(vip) = &profile.vip {
        tracing::info!(
            "deactivate_profile: Removing VIP {}/{} from {}",
            vip.address,
            vip.netmask,
            vip.interface
        );
        state.send_progress(
            &operation_id,
            "deactivate_profile",
            Some(&profile.name),
            0,
            "Removing VIP...",
            false,
            None,
        );
        let vip_cmd = format!(
            "ip addr del {}/{} dev {} 2>/dev/null || true",
            vip.address, vip.netmask, vip.interface
        );
        let vip_output = run_shell_command(
            &vip_cmd,
            &format!(
                "Remove VIP {}/{} from {}",
                vip.address, vip.netmask, vip.interface
            ),
        )
        .await;
        tracing::info!("deactivate_profile: VIP remove result: {:?}", vip_output);
    }

    state.send_progress(
        &operation_id,
        "deactivate_profile",
        Some(&profile.name),
        20,
        "Stopping services...",
        false,
        None,
    );

    // Stop services in reverse order and wait for them to fully stop
    tracing::info!(
        "deactivate_profile: Stopping {} services in reverse order: {:?}",
        profile.promoter.services.len(),
        profile.promoter.services.iter().rev().collect::<Vec<_>>()
    );
    let systemd = SystemdController::new().await?;
    for service in profile.promoter.services.iter().rev() {
        tracing::info!("deactivate_profile: Stopping service '{}'", service);
        if let Err(e) = systemd.stop(service).await {
            tracing::warn!(
                "deactivate_profile: Failed to stop service {}: {}",
                service,
                e
            );
        } else {
            tracing::info!("deactivate_profile: Service '{}' stopped", service);
        }
    }

    // Wait a moment for services to release file handles
    tracing::info!("deactivate_profile: Waiting 500ms for services to release file handles");
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Kill any remaining processes using the mount point (except our own process)
    state.send_progress(
        &operation_id,
        "deactivate_profile",
        Some(&profile.name),
        40,
        "Releasing mount point...",
        false,
        None,
    );
    // Use lsof to find PIDs and kill them individually, excluding our process
    let my_pid = std::process::id();
    tracing::info!(
        "deactivate_profile: Killing processes using {} (excluding pid={})",
        profile.mount_point,
        my_pid
    );
    let kill_cmd = format!(
        "for pid in $(lsof -t {} 2>/dev/null | grep -v {}); do kill -9 $pid 2>/dev/null || true; done",
        profile.mount_point, my_pid
    );
    let kill_output = run_shell_command(
        &kill_cmd,
        &format!("Kill processes using mount point {}", profile.mount_point),
    )
    .await;
    tracing::info!("deactivate_profile: Kill result: {:?}", kill_output);

    // Wait for processes to terminate
    tracing::info!("deactivate_profile: Waiting 500ms for processes to terminate");
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    state.send_progress(
        &operation_id,
        "deactivate_profile",
        Some(&profile.name),
        50,
        "Unmounting DRBD device...",
        false,
        None,
    );

    // Unmount the DRBD device - try multiple times if needed
    let mut unmounted = false;
    for attempt in 1..=3 {
        let umount_cmd = format!("umount {}", profile.mount_point);
        tracing::info!(
            "deactivate_profile: Unmount attempt {}: '{}'",
            attempt,
            umount_cmd
        );
        let output = run_shell_command(
            &umount_cmd,
            &format!("Unmount {} (attempt {})", profile.mount_point, attempt),
        )
        .await?;
        tracing::info!(
            "deactivate_profile: Unmount result: success={}, stderr='{}'",
            output.success(),
            output.stderr.trim()
        );
        if output.success() {
            unmounted = true;
            tracing::info!(
                "deactivate_profile: Unmount succeeded on attempt {}",
                attempt
            );
            break;
        }
        tracing::warn!(
            "deactivate_profile: Unmount attempt {} failed: {}",
            attempt,
            output.stderr
        );
        if attempt < 3 {
            // Force kill processes and retry (excluding our own process)
            tracing::info!(
                "deactivate_profile: Retrying kill of processes using {}",
                profile.mount_point
            );
            let retry_kill_cmd = format!(
                "for pid in $(lsof -t {} 2>/dev/null | grep -v {}); do kill -9 $pid 2>/dev/null || true; done",
                profile.mount_point, my_pid
            );
            let _ = run_shell_command(
                &retry_kill_cmd,
                &format!(
                    "Retry killing processes using mount point {}",
                    profile.mount_point
                ),
            )
            .await;
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }

    // Try lazy unmount as last resort
    if !unmounted {
        tracing::info!("deactivate_profile: Regular unmount failed, trying lazy unmount");
        let lazy_umount_cmd = format!("umount -l {}", profile.mount_point);
        tracing::info!("deactivate_profile: Running '{}'", lazy_umount_cmd);
        let output = run_shell_command(
            &lazy_umount_cmd,
            &format!("Lazy unmount {}", profile.mount_point),
        )
        .await?;
        tracing::info!(
            "deactivate_profile: Lazy unmount result: success={}, stderr='{}'",
            output.success(),
            output.stderr.trim()
        );
        if !output.success() {
            tracing::error!("deactivate_profile: Lazy unmount failed: {}", output.stderr);
            state.send_progress(
                &operation_id,
                "deactivate_profile",
                Some(&profile.name),
                60,
                &format!("Failed to unmount: {}", output.stderr),
                true,
                Some(false),
            );
            state.send_notification(
                NotificationLevel::Error,
                "Deactivation Failed",
                &format!(
                    "Failed to unmount '{}': {}",
                    profile.mount_point, output.stderr
                ),
            );
            return Err(AppError::Drbd(format!(
                "Failed to unmount {}: {}",
                profile.mount_point, output.stderr
            )));
        }
        // Wait for lazy unmount to complete
        tracing::info!("deactivate_profile: Waiting 1000ms for lazy unmount to complete");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    state.send_progress(
        &operation_id,
        "deactivate_profile",
        Some(&profile.name),
        70,
        "Demoting DRBD resource...",
        false,
        None,
    );

    // Demote DRBD resource
    let demote_cmd = format!("drbdadm secondary {}", profile.resource_name);
    tracing::info!("deactivate_profile: Running '{}'", demote_cmd);
    let output = run_shell_command(
        &demote_cmd,
        &format!("Demote DRBD resource {}", profile.resource_name),
    )
    .await?;
    tracing::info!(
        "deactivate_profile: Demote result: success={}, stderr='{}'",
        output.success(),
        output.stderr.trim()
    );
    if !output.success() {
        tracing::error!("deactivate_profile: Demote failed: {}", output.stderr);
        state.send_progress(
            &operation_id,
            "deactivate_profile",
            Some(&profile.name),
            80,
            &format!("Failed to demote: {}", output.stderr),
            true,
            Some(false),
        );
        state.send_notification(
            NotificationLevel::Error,
            "Deactivation Failed",
            &format!("Failed to demote '{}': {}", profile.name, output.stderr),
        );
        return Err(AppError::Drbd(format!(
            "Failed to demote resource: {}",
            output.stderr
        )));
    }

    tracing::info!(
        "deactivate_profile: Profile '{}' deactivated successfully",
        profile.name
    );
    state.send_progress(
        &operation_id,
        "deactivate_profile",
        Some(&profile.name),
        100,
        "Profile deactivated successfully",
        true,
        Some(true),
    );
    state.send_notification(
        NotificationLevel::Info,
        "Profile Deactivated",
        &format!("HA profile '{}' is now standby", profile.name),
    );

    // Get updated status
    get_profile_status(State(state), Path(id)).await
}

/// GET /api/v1/ha/reactor/status
/// Get drbd-reactor service status
pub async fn reactor_status(
    State(_state): State<Arc<AppState>>,
) -> AppResult<Json<serde_json::Value>> {
    let systemd = SystemdController::new().await?;
    let status = systemd.status("drbd-reactor.service").await?;
    Ok(Json(serde_json::json!({
        "service": "drbd-reactor.service",
        "active_state": status.active_state,
        "sub_state": status.sub_state,
        "running": status.is_running(),
        "description": status.description
    })))
}

/// Query parameters for reactor log retrieval
#[derive(Debug, Deserialize)]
pub struct ReactorLogsQuery {
    /// Number of lines to retrieve (default: 100, max: 1000)
    #[serde(default = "default_reactor_log_lines")]
    pub lines: u32,
    /// Filter logs since this time (e.g., "1h", "30m", "2024-01-15")
    pub since: Option<String>,
}

fn default_reactor_log_lines() -> u32 {
    100
}

/// Response for reactor logs
#[derive(Serialize)]
pub struct ReactorLogsResponse {
    pub service: String,
    pub lines: Vec<String>,
    pub total_lines: usize,
}

/// GET /api/v1/ha/reactor/logs
/// Get drbd-reactor logs from journalctl
pub async fn reactor_logs(
    Query(query): Query<ReactorLogsQuery>,
) -> AppResult<Json<ReactorLogsResponse>> {
    // Limit lines to prevent excessive output
    let lines = query.lines.min(1000);

    // Build journalctl command
    let mut cmd = format!("journalctl -u drbd-reactor.service -n {} --no-pager", lines);

    // Add time filter if specified
    if let Some(since) = &query.since {
        // Validate since format (simple check)
        if since
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == ':' || c == ' ')
        {
            cmd.push_str(&format!(" --since '{}'", since));
        }
    }

    let output = run_shell_command(&cmd, "Get drbd-reactor logs").await?;

    let log_lines: Vec<String> = output.stdout.lines().map(|s| s.to_string()).collect();

    Ok(Json(ReactorLogsResponse {
        service: "drbd-reactor.service".to_string(),
        total_lines: log_lines.len(),
        lines: log_lines,
    }))
}

/// Query parameters for service listing
#[derive(Debug, Deserialize)]
pub struct ListServicesQuery {
    /// Include system services (default: false)
    #[serde(default)]
    pub include_system: bool,
}

/// Response for service list
#[derive(Serialize)]
pub struct ServiceListResponse {
    pub services: Vec<ServiceInfo>,
}

/// Response for service files list
#[derive(Serialize)]
pub struct ServiceFileListResponse {
    pub services: Vec<ServiceFileInfo>,
}

/// GET /api/v1/services
/// List all running systemd services (for HA service selection)
pub async fn list_services(
    Query(query): Query<ListServicesQuery>,
) -> AppResult<Json<ServiceListResponse>> {
    let systemd = SystemdController::new().await?;
    let services = systemd.list_services(query.include_system).await?;
    Ok(Json(ServiceListResponse { services }))
}

/// GET /api/v1/services/available
/// List all available service unit files (including disabled ones)
pub async fn list_available_services(
    Query(query): Query<ListServicesQuery>,
) -> AppResult<Json<ServiceFileListResponse>> {
    let systemd = SystemdController::new().await?;
    let services = systemd.list_service_files(query.include_system).await?;
    Ok(Json(ServiceFileListResponse { services }))
}

/// Request for reactor reload
#[derive(Debug, Deserialize)]
pub struct ReactorReloadRequest {
    /// Action: "reload" or "restart"
    #[serde(default = "default_reload_action")]
    pub action: String,
}

fn default_reload_action() -> String {
    "reload".to_string()
}

/// Response for reactor reload
#[derive(Serialize)]
pub struct ReactorReloadResponse {
    /// Local node result
    pub local: NodeReloadResult,
    /// Remote node results
    pub remote_nodes: Vec<NodeReloadResult>,
    /// Overall message
    pub message: String,
}

#[derive(Serialize)]
pub struct NodeReloadResult {
    pub hostname: String,
    pub success: bool,
    pub error: Option<String>,
}

/// POST /api/v1/ha/reactor/reload
/// Reload or restart drbd-reactor on all cluster nodes
pub async fn reload_reactor(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ReactorReloadRequest>,
) -> AppResult<Json<ReactorReloadResponse>> {
    let action = match request.action.as_str() {
        "restart" => "restart",
        _ => "reload",
    };

    let mut results = Vec::new();

    // Step 1: Reload on local node first
    let local_hostname = gethostname::gethostname().to_string_lossy().to_string();
    let local_cmd = format!(
        "systemctl daemon-reload && systemctl {} drbd-reactor",
        action
    );

    let (local_success, local_error) = match run_shell_command(
        &local_cmd,
        &format!("{} drbd-reactor on local node", action),
    )
    .await
    {
        Ok(output) => (
            output.success(),
            if output.success() {
                None
            } else {
                Some(output.stderr)
            },
        ),
        Err(e) => (false, Some(e.to_string())),
    };

    let local_node_result = NodeReloadResult {
        hostname: local_hostname.clone(),
        success: local_success,
        error: local_error,
    };

    // Step 2: Reload on remote nodes
    let nodes = state.db.get_all_nodes()?;
    let remote_cmd = format!(
        "systemctl daemon-reload && systemctl {} drbd-reactor",
        action
    );

    for node in nodes {
        if node.is_local {
            continue;
        }

        // Get credential
        let credential = Some(crate::core::SshCredential::Password("ignored".to_string()));

        let result = if let Some(cred) = credential {
            match state
                .ssh_manager
                .execute(&node.ip, node.ssh_port, &node.ssh_user, &cred, &remote_cmd)
                .await
            {
                Ok(output) => NodeReloadResult {
                    hostname: node.hostname.clone(),
                    success: output.success(),
                    error: if output.success() {
                        None
                    } else {
                        Some(output.stderr)
                    },
                },
                Err(e) => NodeReloadResult {
                    hostname: node.hostname.clone(),
                    success: false,
                    error: Some(e.to_string()),
                },
            }
        } else {
            NodeReloadResult {
                hostname: node.hostname.clone(),
                success: false,
                error: Some("No credentials available".to_string()),
            }
        };

        results.push(result);
    }

    let success_count =
        results.iter().filter(|r| r.success).count() + if local_success { 1 } else { 0 };
    let total_count = results.len() + 1;

    let message = if success_count == total_count {
        format!(
            "Successfully {}ed drbd-reactor on all {} nodes",
            action, total_count
        )
    } else {
        format!(
            "{}ed drbd-reactor on {}/{} nodes",
            action, success_count, total_count
        )
    };

    tracing::info!("{}", message);

    Ok(Json(ReactorReloadResponse {
        local: local_node_result,
        remote_nodes: results,
        message,
    }))
}

/// GET /api/v1/ha/unmanaged
/// List discovered but unmanaged profiles
#[utoipa::path(
    get,
    path = "/api/v1/ha/unmanaged",
    responses(
        (status = 200, description = "List of unmanaged profiles", body = [HaProfile])
    )
)]
pub async fn list_unmanaged_profiles(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<HaProfile>>> {
    tracing::info!("Scanning for unmanaged HA profiles");
    let discovered = match ReactorDiscovery::scan(None) {
        Ok(profiles) => {
            tracing::info!("Found {} profiles in /etc/drbd-reactor.d", profiles.len());
            profiles
        }
        Err(e) => {
            tracing::error!("Failed to scan ReactorDiscovery: {}", e);
            return Err(AppError::Internal(e.to_string()));
        }
    };

    let existing_profiles = state.db.get_all_ha_profiles()?;
    tracing::info!(
        "Found {} existing profiles in database",
        existing_profiles.len()
    );

    let unmanaged: Vec<HaProfile> = discovered
        .into_iter()
        .filter(|d| {
            let is_unmanaged = !existing_profiles.iter().any(|e| e.name == d.name);
            if is_unmanaged {
                tracing::info!("Profile '{}' is unmanaged", d.name);
            }
            is_unmanaged
        })
        .collect();

    tracing::info!("Returning {} unmanaged profiles", unmanaged.len());
    Ok(Json(unmanaged))
}

/// Request to import profiles
#[derive(Debug, Deserialize, ToSchema)]
pub struct ImportProfilesRequest {
    pub names: Vec<String>,
}

/// Response for import
#[derive(Serialize, ToSchema)]
pub struct ImportProfilesResponse {
    pub imported: Vec<String>,
    pub failed: Vec<String>,
}

/// POST /api/v1/ha/import
/// Import unmanaged profiles
#[utoipa::path(
    post,
    path = "/api/v1/ha/import",
    tag = "ha",
    request_body = ImportProfilesRequest,
    responses(
        (status = 200, description = "Import result", body = ImportProfilesResponse)
    )
)]
pub async fn import_profiles(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ImportProfilesRequest>,
) -> AppResult<Json<ImportProfilesResponse>> {
    let discovered = ReactorDiscovery::scan(None).map_err(|e| AppError::Internal(e.to_string()))?;
    let existing_profiles = state.db.get_all_ha_profiles()?;

    let mut imported = Vec::new();
    let mut failed = Vec::new();

    for name in req.names {
        if existing_profiles.iter().any(|e| e.name == name) {
            failed.push(format!("{}: Already managed", name));
            continue;
        }

        if let Some(profile) = discovered.iter().find(|d| d.name == name) {
            // Mark status as Active since it's already on disk
            let mut profile_to_save = profile.clone();
            profile_to_save.status = HaProfileStatus::Active;

            if let Err(e) = state.db.insert_ha_profile(&profile_to_save) {
                failed.push(format!("{}: DB error: {}", name, e));
            } else {
                imported.push(name);
            }
        } else {
            failed.push(format!("{}: Not found in discovered profiles", name));
        }
    }

    Ok(Json(ImportProfilesResponse { imported, failed }))
}

/// Request for evicting an HA profile from a node
#[derive(Debug, Deserialize)]
pub struct EvictProfileRequest {
    /// Target node hostname or ID to evict from (optional, defaults to local node)
    pub node: Option<String>,
    /// Delay in seconds to wait for peer takeover (default: 20)
    #[serde(default = "default_evict_delay")]
    pub delay: u32,
    /// Keep the target masked after eviction (prevents automatic failback)
    #[serde(default)]
    pub keep_masked: bool,
    /// Force eviction even with warnings
    #[serde(default)]
    pub force: bool,
}

fn default_evict_delay() -> u32 {
    20
}

/// Response for evict operation
#[derive(Serialize)]
pub struct EvictProfileResponse {
    pub success: bool,
    pub node: String,
    pub profile: String,
    pub message: String,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

/// POST /api/v1/ha/profiles/:id/evict
/// Evict an HA profile from a node (triggers failover to another node)
pub async fn evict_profile(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
    Json(request): Json<EvictProfileRequest>,
) -> AppResult<Json<EvictProfileResponse>> {
    // Find the profile
    let profile = state
        .db
        .get_ha_profile(&id_or_name)?
        .or(state.db.get_ha_profile_by_name(&id_or_name)?)
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    // Build drbd-reactorctl evict command
    let mut evict_cmd = format!("drbd-reactorctl evict {}", profile.name);

    if request.delay != 20 {
        evict_cmd.push_str(&format!(" --delay {}", request.delay));
    }
    if request.keep_masked {
        evict_cmd.push_str(" --keep-masked");
    }
    if request.force {
        evict_cmd.push_str(" --force");
    }

    // Determine target node - must be the active node for evict to work
    let target_node: crate::models::Node = if let Some(ref node_id) = request.node {
        // Find the node by ID or hostname
        let nodes = state.db.get_all_nodes()?;
        nodes
            .into_iter()
            .find(|n| n.id == *node_id || n.hostname == *node_id)
            .ok_or_else(|| AppError::NotFound(format!("Node {} not found", node_id)))?
    } else {
        // Find the active node from drbd-reactorctl status
        let active_node_name = get_active_node(&profile.name).await;

        if let Some(active_hostname) = active_node_name {
            // Find the node by hostname
            let nodes = state.db.get_all_nodes()?;
            nodes
                .into_iter()
                .find(|n| n.hostname == active_hostname)
                .ok_or_else(|| {
                    AppError::NotFound(format!(
                        "Active node '{}' not found in database. Please add it first.",
                        active_hostname
                    ))
                })?
        } else {
            return Err(AppError::Validation(
                "No active node found for this profile. Cannot evict.".to_string(),
            ));
        }
    };

    let operation_id = uuid::Uuid::new_v4().to_string();
    state.send_progress(
        &operation_id,
        "evict_profile",
        Some(&profile.name),
        0,
        &format!("Evicting from node {}...", target_node.hostname),
        false,
        None,
    );

    tracing::info!(
        "Evicting HA profile {} from node {} (delay: {}s, keep_masked: {}, force: {})",
        profile.name,
        target_node.hostname,
        request.delay,
        request.keep_masked,
        request.force
    );

    // Execute evict command
    let (success, stdout, stderr) = if target_node.is_local {
        // Execute locally
        let output = run_shell_command(
            &evict_cmd,
            &format!("Evict HA profile {} from local node", profile.name),
        )
        .await?;
        (output.success(), Some(output.stdout), Some(output.stderr))
    } else {
        // Execute on remote node via SSH
        let credential = Some(crate::core::SshCredential::Password("ignored".to_string()));

        // Prepend sudo for remote execution to ensure permissions for systemd operations
        let remote_cmd = format!("sudo {}", evict_cmd);

        if let Some(cred) = credential {
            match state
                .ssh_manager
                .execute(
                    &target_node.ip,
                    target_node.ssh_port,
                    &target_node.ssh_user,
                    &cred,
                    &remote_cmd,
                )
                .await
            {
                Ok(output) => (output.success(), Some(output.stdout), Some(output.stderr)),
                Err(e) => {
                    return Err(AppError::Ssh(format!(
                        "Failed to execute evict on {}: {}",
                        target_node.hostname, e
                    )));
                }
            }
        } else {
            return Err(AppError::Ssh(format!(
                "No credentials available for node {}",
                target_node.hostname
            )));
        }
    };

    let message = if success {
        state.send_progress(
            &operation_id,
            "evict_profile",
            Some(&profile.name),
            100,
            &format!(
                "Evicted from {}. Failover in progress...",
                target_node.hostname
            ),
            true,
            Some(true),
        );
        state.send_notification(
            NotificationLevel::Warning,
            "Profile Evicted",
            &format!(
                "HA profile '{}' evicted from {}. Failover initiated.",
                profile.name, target_node.hostname
            ),
        );
        format!(
            "Successfully evicted {} from node {}. Another node should take over within {} seconds.",
            profile.name, target_node.hostname, request.delay
        )
    } else {
        state.send_progress(
            &operation_id,
            "evict_profile",
            Some(&profile.name),
            100,
            &format!(
                "Evict failed: {}",
                stderr.as_deref().unwrap_or("unknown error")
            ),
            true,
            Some(false),
        );
        state.send_notification(
            NotificationLevel::Error,
            "Evict Failed",
            &format!(
                "Failed to evict '{}' from {}",
                profile.name, target_node.hostname
            ),
        );
        format!(
            "Failed to evict {} from node {}: {}",
            profile.name,
            target_node.hostname,
            stderr.as_deref().unwrap_or("unknown error")
        )
    };

    tracing::info!("{}", message);

    Ok(Json(EvictProfileResponse {
        success,
        node: target_node.hostname,
        profile: profile.name,
        message,
        stdout,
        stderr,
    }))
}

/// Request for adding VIP to a profile
#[derive(Debug, Deserialize)]
pub struct AddVipRequest {
    pub address: String,
    pub netmask: u8,
    pub interface: String,
}

/// Response for VIP operations
#[derive(Serialize)]
pub struct VipOperationResponse {
    pub message: String,
}

/// POST /api/v1/ha/profiles/:id/vip
/// Add VIP configuration to an HA profile
pub async fn add_vip(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
    Json(req): Json<AddVipRequest>,
) -> AppResult<Json<VipOperationResponse>> {
    // Validate inputs
    validator::validate_ip_address(&req.address)?;
    validator::validate_netmask(req.netmask)?;

    // Find the profile
    let mut profile = state
        .db
        .get_ha_profile(&id_or_name)?
        .or(state.db.get_ha_profile_by_name(&id_or_name)?)
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    // Check if VIP already exists
    if profile.vip.is_some() {
        return Err(AppError::Conflict(
            "VIP already configured for this profile. Remove existing VIP first.".to_string(),
        ));
    }

    // Create VIP config
    let vip = crate::models::VipConfig {
        address: req.address.clone(),
        netmask: req.netmask,
        interface: req.interface.clone(),
    };

    // Update profile with VIP
    profile.vip = Some(vip.clone());

    // Regenerate promoter configuration with VIP
    let config_gen = ConfigGenerator::new()?;
    let promoter_config = ConfigGenerator::promoter_from_profile(&profile);
    let config_content = config_gen.generate_promoter(&promoter_config)?;
    let config_path = ConfigPaths::promoter_path(&profile.name);

    // Write updated promoter configuration
    tokio::fs::write(&config_path, &config_content)
        .await
        .map_err(|e| AppError::Config(format!("Failed to write promoter config: {}", e)))?;

    // Sync to remote nodes
    let sync_config = HaSyncConfig {
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

    // Update database
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
/// Remove VIP configuration from an HA profile
pub async fn remove_vip(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
) -> AppResult<Json<VipOperationResponse>> {
    // Find the profile
    let mut profile = state
        .db
        .get_ha_profile(&id_or_name)?
        .or(state.db.get_ha_profile_by_name(&id_or_name)?)
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id_or_name)))?;

    // Check if VIP exists
    let old_vip = profile
        .vip
        .clone()
        .ok_or_else(|| AppError::Validation("No VIP configured for this profile".to_string()))?;

    // If profile is active, remove the VIP from the network interface first
    if profile.status == crate::models::HaProfileStatus::Active {
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

    // Remove VIP from profile
    profile.vip = None;

    // Regenerate promoter configuration without VIP
    let config_gen = ConfigGenerator::new()?;
    let promoter_config = ConfigGenerator::promoter_from_profile(&profile);
    let config_content = config_gen.generate_promoter(&promoter_config)?;
    let config_path = ConfigPaths::promoter_path(&profile.name);

    // Write updated promoter configuration
    tokio::fs::write(&config_path, &config_content)
        .await
        .map_err(|e| AppError::Config(format!("Failed to write promoter config: {}", e)))?;

    // Sync to remote nodes
    let sync_config = HaSyncConfig {
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

    // Update database
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

/// Parse drbdadm status output into structured format
/// Example output:
/// ```
/// ```
/// # use crate::api::ha::{parse_drbdadm_status, DrbdResourceStatus}; // Import necessary items
/// let output = r#"
/// mongodb-data role:Primary
///   disk:UpToDate open:yes
///   gui02 role:Secondary
///     peer-disk:UpToDate
///   gui03 role:Secondary
///     peer-disk:UpToDate
/// "#;
/// let resource_name = "mongodb-data";
/// let status = parse_drbdadm_status(output, resource_name).unwrap();
/// assert_eq!(status.resource, "mongodb-data");
/// assert_eq!(status.role, "Primary");
/// assert_eq!(status.disk, "UpToDate");
/// assert_eq!(status.open, true);
/// assert_eq!(status.peers.len(), 2);
/// assert_eq!(status.peers[0].name, "gui02");
/// assert_eq!(status.peers[0].role, "Secondary");
/// assert_eq!(status.peers[0].peer_disk, "UpToDate");
/// ```
fn parse_drbdadm_status(output: &str, resource_name: &str) -> Option<DrbdResourceStatus> {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return None;
    }

    // First line: "mongodb-data role:Primary"
    let first_line = lines.first()?;
    if !first_line.starts_with(resource_name) {
        return None;
    }

    // Parse role from first line
    let role = first_line
        .split_whitespace()
        .find(|s| s.starts_with("role:"))
        .and_then(|s| s.strip_prefix("role:"))
        .unwrap_or("Unknown")
        .to_string();

    // Parse local disk status and open state from second line
    // "  disk:UpToDate open:yes"
    let (disk, open) = if lines.len() > 1 {
        let disk_line = lines[1].trim();
        let disk = disk_line
            .split_whitespace()
            .find(|s| s.starts_with("disk:"))
            .and_then(|s| s.strip_prefix("disk:"))
            .unwrap_or("Unknown")
            .to_string();
        let open = disk_line
            .split_whitespace()
            .find(|s| s.starts_with("open:"))
            .and_then(|s| s.strip_prefix("open:"))
            .map(|s| s == "yes")
            .unwrap_or(false);
        (disk, open)
    } else {
        ("Unknown".to_string(), false)
    };

    // Parse peer nodes
    let mut peers = Vec::new();
    let mut i = 2;
    while i < lines.len() {
        let line = lines[i].trim();

        // Peer line format: "gui02 role:Secondary" or "gui02 role:Secondary connection:Connected"
        if !line.starts_with("peer-disk:") && !line.is_empty() && !line.starts_with("disk:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if !parts.is_empty() {
                let peer_name = parts[0].to_string();

                // Skip if it looks like a status field rather than a hostname
                if peer_name.contains(':') {
                    i += 1;
                    continue;
                }

                let peer_role = parts
                    .iter()
                    .find(|s| s.starts_with("role:"))
                    .and_then(|s| s.strip_prefix("role:"))
                    .unwrap_or("Unknown")
                    .to_string();

                let connection = parts
                    .iter()
                    .find(|s| s.starts_with("connection:"))
                    .and_then(|s| s.strip_prefix("connection:"))
                    .map(|s| s.to_string());

                let replication = parts
                    .iter()
                    .find(|s| s.starts_with("replication:"))
                    .and_then(|s| s.strip_prefix("replication:"))
                    .map(|s| s.to_string());

                // Next line should have peer-disk
                let peer_disk = if i + 1 < lines.len() {
                    let next_line = lines[i + 1].trim();
                    if next_line.starts_with("peer-disk:") {
                        next_line
                            .split_whitespace()
                            .find(|s| s.starts_with("peer-disk:"))
                            .and_then(|s| s.strip_prefix("peer-disk:"))
                            .unwrap_or("Unknown")
                            .to_string()
                    } else {
                        "Unknown".to_string()
                    }
                } else {
                    "Unknown".to_string()
                };

                peers.push(DrbdPeerStatus {
                    name: peer_name,
                    role: peer_role,
                    peer_disk,
                    connection,
                    replication,
                });
            }
        }
        i += 1;
    }

    Some(DrbdResourceStatus {
        resource: resource_name.to_string(),
        role,
        disk,
        open,
        peers,
    })
}

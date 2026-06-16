//! DRBD resource management API handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use tracing::info;
use utoipa::ToSchema;

use crate::core::{
    drbd_cmd::{parse_drbd_status, DrbdCmd, ResourceStatus},
    run_shell_command,
    safety::SafetyChecker,
    transaction::NodeTarget,
    validator, DrbdConfigGenerator as ConfigGenerator, DrbdConfigPaths as ConfigPaths, NodeConfig,
    ResourceConfig,
};
use crate::error::{AppError, AppResult};
use crate::models::{
    CreateFilesystemRequest, CreateResourceRequest, MountRequest, ResourceAction,
    ResourceActionRequest,
};
use crate::state::{AppState, NotificationLevel};
use drbd_utils::allocate_minor;
use lvm_utils::LvmCmd;
use zfs_utils::ZfsCmd;

/// Response for resource list
#[derive(Serialize, ToSchema)]
pub struct ResourceListResponse {
    pub resources: Vec<ResourceStatus>,
}

/// Response for resource creation
#[derive(Serialize, ToSchema)]
pub struct ResourceCreateResponse {
    pub name: String,
    pub message: String,
    pub config_path: String,
}

/// Response for resource action
#[derive(Serialize, ToSchema)]
pub struct ResourceActionResponse {
    pub resource: String,
    pub action: String,
    pub success: bool,
    pub message: Option<String>,
}

/// GET /api/v1/resources
/// List all DRBD resources
#[utoipa::path(
    get,
    path = "/api/v1/resources",
    tag = "resources",
    summary = "List all DRBD resources",
    responses(
        (status = 200, description = "List of DRBD resources", body = ResourceListResponse)
    )
)]
pub async fn list_resources(
    State(_state): State<Arc<AppState>>,
) -> AppResult<Json<ResourceListResponse>> {
    let cmd = DrbdCmd::status_cmd();
    let output = run_shell_command(&cmd, "List all DRBD resources").await?;

    let resources = if output.success() {
        parse_drbd_status(&output.stdout)?
    } else {
        Vec::new()
    };

    Ok(Json(ResourceListResponse { resources }))
}

/// GET /api/v1/resources/:name
/// Get status of a specific resource
#[utoipa::path(
    get,
    path = "/api/v1/resources/{name}",
    tag = "resources",
    params(
        ("name" = String, Path, description = "Resource name")
    ),
    responses(
        (status = 200, description = "Resource status", body = ResourceStatus),
        (status = 404, description = "Resource not found")
    )
)]
pub async fn get_resource(
    State(_state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> AppResult<Json<ResourceStatus>> {
    validator::validate_resource_name(&name)?;

    let cmd = DrbdCmd::resource_status_cmd(&name)?;
    let output = run_shell_command(&cmd, &format!("Get status for DRBD resource {}", name)).await?;

    if !output.success() {
        return Err(AppError::NotFound(format!("Resource {} not found", name)));
    }

    let resources = parse_drbd_status(&output.stdout)?;
    resources
        .into_iter()
        .next()
        .ok_or_else(|| AppError::NotFound(format!("Resource {} not found", name)))
        .map(Json)
}

/// POST /api/v1/resources
/// Create a new DRBD resource
#[utoipa::path(
    post,
    path = "/api/v1/resources",
    tag = "resources",
    request_body = CreateResourceRequest,
    responses(
        (status = 201, description = "Resource created successfully", body = ResourceCreateResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Resource already exists")
    )
)]
pub async fn create_resource(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateResourceRequest>,
) -> AppResult<(StatusCode, Json<ResourceCreateResponse>)> {
    let operation_id = uuid::Uuid::new_v4().to_string();

    // Send initial progress
    state.send_progress(
        &operation_id,
        "create_resource",
        Some(&req.name),
        0,
        "Validating inputs...",
        false,
        None,
    );

    // Validate inputs
    validator::validate_resource_name(&req.name)?;
    validator::validate_port(req.port)?;

    // Auto-allocate minor number if not provided or is 0
    let minor = if req.minor == 0 {
        let allocated = allocate_minor().await;
        validator::validate_minor(allocated)?;
        allocated
    } else {
        // User specified a minor, validate it's available
        validator::validate_minor(req.minor)?;
        validator::validate_minor_available(req.minor).await?;
        req.minor
    };

    if req.node_disks.is_empty() {
        return Err(AppError::Validation(
            "At least one node disk must be specified".to_string(),
        ));
    }

    // Validate all block devices
    for disk in req.node_disks.values() {
        validator::validate_block_device(disk)?;
    }

    // Get node information from store
    // Removed unused creds variable

    // Build node configs and collect targets for safety checks
    let mut node_configs: Vec<NodeConfig> = Vec::new();
    let mut targets: Vec<NodeTarget> = Vec::new();
    let mut local_disks: Vec<String> = Vec::new();
    let mut remote_checks: Vec<(String, u16, String, crate::core::SshCredential, String)> =
        Vec::new();

    // Prepare LVM parameters if requested
    let (lvm_path, lvm_vg_name, lvm_lv_name, lvm_lv_size) = if req.init_lvm {
        let vg_name = req.lvm_vg_name.as_ref().ok_or_else(|| {
            AppError::Validation("lvm_vg_name is required when init_lvm is true".to_string())
        })?;
        let lv_name = req.lvm_lv_name.as_ref().unwrap_or(&req.name);
        let lv_size = req.lvm_lv_size.as_deref().unwrap_or("100%FREE"); // Default to full size

        let path = format!("/dev/{}/{}", vg_name, lv_name);
        (
            Some(path),
            Some(vg_name.clone()),
            Some(lv_name.clone()),
            Some(lv_size.to_string()),
        )
    } else {
        (None, None, None, None)
    };

    // Prepare ZFS parameters if requested
    let (zfs_path, zfs_pool_name, zfs_volume_name, zfs_volume_size_gb) =
        if let Some(storage_type) = &req.storage_type {
            if storage_type == "zfs" {
                let pool_name = req.zfs_pool_name.as_ref().ok_or_else(|| {
                    AppError::Validation(
                        "zfs_pool_name is required when storage_type is 'zfs'".to_string(),
                    )
                })?;
                let volume_name = req.zfs_volume_name.as_ref().unwrap_or(&req.name);
                let volume_size_gb = req.zfs_volume_size_gb.ok_or_else(|| {
                    AppError::Validation(
                        "zfs_volume_size_gb is required when storage_type is 'zfs'".to_string(),
                    )
                })?;

                let path = format!("/dev/zvol/{}/{}", pool_name, volume_name);
                (
                    Some(path),
                    Some(pool_name.clone()),
                    Some(volume_name.clone()),
                    Some(volume_size_gb),
                )
            } else {
                (None, None, None, None)
            }
        } else {
            (None, None, None, None)
        };

    // Check if the generated device name conflicts with existing resources
    // Generate a temporary config just to get the device name
    let temp_nodes: Vec<NodeConfig> = req
        .node_disks
        .iter()
        .enumerate()
        .map(|(i, (_node_id, disk))| {
            // Use placeholder values for now, we'll validate nodes later
            NodeConfig {
                hostname: format!("node{}", i),
                ip: format!("192.168.1.{}", i + 1),
                disk: disk.clone(),
                node_id: i as u32,
            }
        })
        .collect();

    // DRBD requires device name to match minor number (e.g., /dev/drbd10 minor 10)
    let temp_config = ResourceConfig {
        name: req.name.clone(),
        port: req.port,
        minor,
        device: format!("/dev/drbd{}", minor),
        nodes: temp_nodes,
        auto_promote: req.auto_promote,
        ..Default::default()
    };

    state.send_progress(
        &operation_id,
        "create_resource",
        Some(&req.name),
        3,
        "Checking device name conflicts...",
        false,
        None,
    );

    validator::validate_device_unique(&temp_config.device, state.drbd_config_dir()).await?;

    tracing::info!(
        "Device name '{}' is available for resource '{}'",
        temp_config.device,
        req.name
    );

    if req.init_lvm || req.storage_type.as_ref().is_some_and(|t| t == "zfs") {
        state.send_progress(
            &operation_id,
            "create_resource",
            Some(&req.name),
            5,
            if req.init_lvm {
                "Initializing LVM on all nodes..."
            } else {
                "Initializing ZFS on all nodes..."
            },
            false,
            None,
        );
    }

    for (node_id, disk) in &req.node_disks {
        let node = state
            .node_store
            .get(node_id)?
            .ok_or_else(|| AppError::NotFound(format!("Node {} not found", node_id)))?;

        // Initialize LVM if requested
        if let (Some(vg_name), Some(lv_name), Some(lv_size)) =
            (&lvm_vg_name, &lvm_lv_name, &lvm_lv_size)
        {
            // Thin pool configuration (default to thin pool)
            let thin_pool_name = req.lvm_thin_pool_name.as_deref().unwrap_or("thinpool");
            let thin_pool_size = req.lvm_thin_pool_size.as_deref().unwrap_or("1G");

            // Execute LVM commands with thin pool support
            // Note: -ff (force) is used to overwrite existing headers if any (careful!)
            // Using -y to assume yes
            let cmds = [
                LvmCmd::pvcreate_cmd(disk),
                LvmCmd::vgcreate_cmd(vg_name, disk),
                // Create thin pool (metadata size is about 1% of pool size by default)
                LvmCmd::create_thin_pool_cmd(vg_name, thin_pool_name, thin_pool_size),
                // Create thin volume from thin pool
                LvmCmd::create_thin_volume_cmd(vg_name, thin_pool_name, lv_name, lv_size),
            ];
            let cmd_str = cmds.join(" && ");

            info!(
                "Initializing LVM with thin pool on {}: {}",
                node.hostname, cmd_str
            );

            if state.is_controller_node(&node) {
                let output =
                    run_shell_command(&cmd_str, "Initialize local LVM with thin pool").await?;
                if !output.success() {
                    return Err(AppError::Internal(format!(
                        "Failed to init LVM with thin pool on local node: {}",
                        output.stderr
                    )));
                }
            } else {
                let cred = crate::core::SshCredential::Password("ignored".to_string());
                // Prepend sudo for remote commands
                let sudo_cmd_str = format!("sudo bash -c '{}'", cmd_str);
                let output = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &cred,
                        &sudo_cmd_str,
                    )
                    .await
                    .map_err(|e| {
                        AppError::Internal(format!(
                            "Failed to connect to remote node {}: {}",
                            node.hostname, e
                        ))
                    })?;

                if !output.success() {
                    return Err(AppError::Internal(format!(
                        "Failed to init LVM with thin pool on remote node {}: {}",
                        node.hostname, output.stderr
                    )));
                }
            }
        }

        // Initialize ZFS if requested
        if let (Some(pool_name), Some(volume_name), Some(volume_size_gb)) =
            (&zfs_pool_name, &zfs_volume_name, &zfs_volume_size_gb)
        {
            // ZFS thin provisioning (sparse volume) by default
            let use_thin = req.zfs_thin_volume;

            // Create ZFS pool if it doesn't exist
            let check_cmd = ZfsCmd::zpool_list_cmd(pool_name);
            let create_cmd = ZfsCmd::zpool_create_cmd(pool_name, disk);
            let pool_cmd = format!("{} || {}", check_cmd, create_cmd);

            // Create ZFS volume with thin provisioning (sparse) or pre-allocated
            let volume_cmd = if use_thin {
                // Sparse volume: -s flag creates a sparse (thin) volume
                ZfsCmd::zfs_create_sparse_volume_cmd(
                    pool_name,
                    volume_name,
                    &volume_size_gb.to_string(),
                )
            } else {
                // Pre-allocated volume (thick)
                ZfsCmd::zfs_create_volume_cmd(pool_name, volume_name, &volume_size_gb.to_string())
            };

            let cmds = [pool_cmd, volume_cmd];
            let cmd_str = cmds.join(" && ");

            info!(
                "Initializing ZFS with thin provisioning={} on {}: {}",
                use_thin, node.hostname, cmd_str
            );

            if state.is_controller_node(&node) {
                let output = run_shell_command(&cmd_str, "Initialize local ZFS").await?;
                if !output.success() {
                    return Err(AppError::Internal(format!(
                        "Failed to init ZFS on local node: {}",
                        output.stderr
                    )));
                }
            } else {
                let cred = crate::core::SshCredential::Password("ignored".to_string());
                // Prepend sudo for remote commands
                let sudo_cmd_str = format!("sudo bash -c '{}'", cmd_str);
                let output = state
                    .ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        &cred,
                        &sudo_cmd_str,
                    )
                    .await
                    .map_err(|e| {
                        AppError::Internal(format!(
                            "Failed to connect to remote node {}: {}",
                            node.hostname, e
                        ))
                    })?;

                if !output.success() {
                    return Err(AppError::Internal(format!(
                        "Failed to init ZFS on remote node {}: {}",
                        node.hostname, output.stderr
                    )));
                }
            }
        }

        // Determine the effective disk path (优先使用存储池路径)
        let effective_disk = if let Some(ref zfs_path) = zfs_path {
            zfs_path.clone()
        } else if let Some(ref lvm_path) = lvm_path {
            lvm_path.clone()
        } else {
            disk.clone()
        };

        node_configs.push(NodeConfig {
            hostname: node.hostname.clone(),
            ip: node.ip.clone(),
            disk: effective_disk.clone(),
            node_id: node_configs.len() as u32,
        });

        // Get credential for remote nodes
        if !state.is_controller_node(&node) {
            // Dummy credential
            let cred = crate::core::SshCredential::Password("ignored".to_string());

            targets.push(NodeTarget {
                host: node.ip.clone(),
                port: node.ssh_port,
                user: node.ssh_user.clone(),
                credential: cred.clone(),
            });
            remote_checks.push((
                node.ip.clone(),
                node.ssh_port,
                node.ssh_user.clone(),
                cred.clone(),
                effective_disk.clone(),
            ));
        } else {
            local_disks.push(effective_disk.clone());
        }
    }

    // Safety checks: Create checker
    let safety_checker = SafetyChecker::new(state.ssh_manager.clone());

    state.send_progress(
        &operation_id,
        "create_resource",
        Some(&req.name),
        10,
        "Verifying network connectivity...",
        false,
        None,
    );

    // Safety check 1: Verify all remote nodes are reachable BEFORE any modification
    if !targets.is_empty() {
        info!(
            "Verifying network connectivity to {} remote node(s)",
            targets.len()
        );
        let connectivity_targets: Vec<_> = targets
            .iter()
            .map(|t| (t.host.clone(), t.port, t.user.clone(), t.credential.clone()))
            .collect();
        safety_checker
            .verify_all_nodes_reachable(&connectivity_targets)
            .await?;
        info!("All remote nodes are reachable");
    }

    state.send_progress(
        &operation_id,
        "create_resource",
        Some(&req.name),
        20,
        "Running safety checks on local devices...",
        false,
        None,
    );

    // Safety check 2: Verify local backing devices are safe to use
    for disk in &local_disks {
        info!("Running safety checks on local device {}", disk);
        let check = safety_checker.check_device_for_drbd(disk).await?;
        if let Some(err) = check.to_error() {
            if req.force {
                tracing::warn!(
                    "Safety check failed for local device {}, but proceeding due to force flag: {}",
                    disk,
                    err
                );
            } else {
                state.send_progress(
                    &operation_id,
                    "create_resource",
                    Some(&req.name),
                    20,
                    &format!("Safety check failed: {}", err),
                    true,
                    Some(false),
                );
                return Err(err);
            }
        }
        for warning in &check.warnings {
            tracing::warn!("Local device {}: {}", disk, warning);
        }
    }

    state.send_progress(
        &operation_id,
        "create_resource",
        Some(&req.name),
        40,
        "Running safety checks on remote devices...",
        false,
        None,
    );

    // Safety check 3: Verify remote backing devices are safe to use
    for (host, port, user, credential, disk) in &remote_checks {
        info!(
            "Running safety checks on remote device {} at {}",
            disk, host
        );
        let check = safety_checker
            .check_remote_device_for_drbd(host, *port, user, credential, disk)
            .await?;
        if let Some(err) = check.to_error() {
            if req.force {
                tracing::warn!(
                    "Safety check failed for remote device {} on {}, but proceeding due to force flag: {}",
                    disk,
                    host,
                    err
                );
            } else {
                state.send_progress(
                    &operation_id,
                    "create_resource",
                    Some(&req.name),
                    40,
                    &format!("Safety check failed: {}", err),
                    true,
                    Some(false),
                );
                return Err(err);
            }
        }
        for warning in &check.warnings {
            tracing::warn!("Remote device {} on {}: {}", disk, host, warning);
        }
    }

    info!("All safety checks passed for resource {}", req.name);
    state.send_progress(
        &operation_id,
        "create_resource",
        Some(&req.name),
        60,
        "Generating DRBD configuration...",
        false,
        None,
    );

    // Generate configuration
    let config_gen = ConfigGenerator::new()?;

    // Build net_options from request or use defaults
    let net_options = if let Some(ref opts) = req.net_options {
        opts.clone()
    } else {
        // Default options from drbd-utils
        let mut opts = std::collections::HashMap::new();
        opts.insert("protocol".to_string(), "C".to_string());
        opts.insert("verify-alg".to_string(), "sha256".to_string());
        opts
    };

    // DRBD requires device name to match minor number (e.g., /dev/drbd10 minor 10)
    let resource_config = ResourceConfig {
        name: req.name.clone(),
        port: req.port,
        minor,
        device: format!("/dev/drbd{}", minor),
        nodes: node_configs,
        auto_promote: false, // Hardcoded as requested
        net_options,
        ..Default::default()
    };

    let config_content = config_gen.generate_drbd_resource(&resource_config)?;
    let config_path = ConfigPaths::drbd_resource_path(&req.name);

    state.send_progress(
        &operation_id,
        "create_resource",
        Some(&req.name),
        70,
        "Writing configuration locally...",
        false,
        None,
    );

    // Write config locally first
    state
        .write_controller_file(&config_path, &config_content)
        .await?;

    info!("create_resource: Written local config to {}", config_path);
    state.send_progress(
        &operation_id,
        "create_resource",
        Some(&req.name),
        80,
        "Syncing configuration to remote nodes...",
        false,
        None,
    );

    // Write to ALL remote nodes in the cluster (not just nodes specified in node_disks)
    // This ensures all nodes have the resource configuration for DRBD to work properly
    let all_nodes = state.node_store.get_all()?;
    info!(
        "create_resource: Found {} total nodes in store",
        all_nodes.len()
    );

    let remote_nodes: Vec<_> = all_nodes
        .iter()
        .filter(|node| !state.is_controller_node(node))
        .collect();
    info!(
        "create_resource: {} remote nodes to sync: {:?}",
        remote_nodes.len(),
        remote_nodes.iter().map(|n| &n.hostname).collect::<Vec<_>>()
    );

    let mut written_hosts: Vec<(String, u16, String, crate::core::SshCredential)> = Vec::new();

    // Removed unused creds acquisition

    for node in &remote_nodes {
        info!(
            "create_resource: Processing node {} ({})",
            node.hostname, node.ip
        );

        // Get credential for remote node
        let credential = crate::core::SshCredential::Password("ignored".to_string());

        info!(
            "create_resource: Writing config to {} via SSH",
            node.hostname
        );
        match state
            .ssh_manager
            .write_file(
                &node.ip,
                node.ssh_port,
                &node.ssh_user,
                &credential,
                &config_path,
                &config_content,
            )
            .await
        {
            Ok(_) => {
                info!(
                    "create_resource: Config written to {}, verifying...",
                    node.hostname
                );
                // Verify the config file was written correctly
                let verify_cmd = format!("test -f '{}' && echo OK", config_path);
                match state
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
                    Ok(output) if output.stdout.contains("OK") => {
                        info!(
                            "create_resource: Config synced and VERIFIED on {}",
                            node.hostname
                        );
                    }
                    Ok(output) => {
                        tracing::warn!(
                            "create_resource: Verification failed on {}: stdout='{}', stderr='{}'",
                            node.hostname,
                            output.stdout,
                            output.stderr
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            "create_resource: Verification command failed on {}: {}",
                            node.hostname,
                            e
                        );
                    }
                }
                written_hosts.push((
                    node.ip.clone(),
                    node.ssh_port,
                    node.ssh_user.clone(),
                    credential,
                ));
            }
            Err(e) => {
                // Rollback: delete config from nodes where it was written
                tracing::error!(
                    "create_resource: Failed to write config to {}: {}",
                    node.hostname,
                    e
                );
                info!(
                    "create_resource: Rolling back {} previously written configs",
                    written_hosts.len()
                );
                for (host, port, user, cred) in &written_hosts {
                    let _ = state
                        .ssh_manager
                        .execute(host, *port, user, cred, &format!("rm -f '{}'", config_path))
                        .await;
                }
                // Also remove local config
                let _ = state.remove_controller_file(&config_path).await;
                return Err(e);
            }
        }
    }

    info!(
        "create_resource: Sync completed, wrote config to {} remote nodes",
        written_hosts.len()
    );

    state.send_progress(
        &operation_id,
        "create_resource",
        Some(&req.name),
        100,
        "Resource created successfully",
        true,
        Some(true),
    );
    state.send_notification(
        NotificationLevel::Success,
        "Resource Created",
        &format!("DRBD resource '{}' created successfully", req.name),
    );

    Ok((
        StatusCode::CREATED,
        Json(ResourceCreateResponse {
            name: req.name,
            message: "Resource configuration created. Run 'up' action to initialize.".to_string(),
            config_path,
        }),
    ))
}

/// POST /api/v1/resources/:name/action
/// Perform an action on a resource (up, down, primary, secondary, etc.)
#[utoipa::path(
    post,
    path = "/api/v1/resources/{name}/action",
    tag = "resources",
    params(
        ("name" = String, Path, description = "Resource name")
    ),
    request_body = ResourceActionRequest,
    responses(
        (status = 200, description = "Action executed", body = ResourceActionResponse),
        (status = 400, description = "Invalid request")
    )
)]
pub async fn resource_action(
    State(_state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<ResourceActionRequest>,
) -> AppResult<Json<ResourceActionResponse>> {
    validator::validate_resource_name(&name)?;

    // Handle split brain recovery specially - it's a sequence of commands
    if matches!(req.action, ResourceAction::RecoverSplitBrain) {
        return recover_split_brain(&name).await;
    }

    let cmd = match req.action {
        ResourceAction::Up => DrbdCmd::up_cmd(&name)?,
        ResourceAction::Down => DrbdCmd::down_cmd(&name)?,
        ResourceAction::Primary => DrbdCmd::primary_cmd(&name, req.force)?,
        ResourceAction::Secondary => DrbdCmd::secondary_cmd(&name)?,
        ResourceAction::Connect => DrbdCmd::connect_cmd(&name)?,
        ResourceAction::Disconnect => DrbdCmd::disconnect_cmd(&name)?,
        ResourceAction::Invalidate => DrbdCmd::invalidate_cmd(&name)?,
        ResourceAction::Verify => DrbdCmd::verify_cmd(&name)?,
        ResourceAction::RecoverSplitBrain => unreachable!(), // handled above
    };

    let action_name = format!("{:?}", req.action).to_lowercase();
    let output = run_shell_command(
        &cmd,
        &format!("Execute DRBD action {} for resource {}", action_name, name),
    )
    .await?;

    Ok(Json(ResourceActionResponse {
        resource: name,
        action: action_name,
        success: output.success(),
        message: if output.success() {
            None
        } else {
            Some(output.stderr)
        },
    }))
}

/// Recover from split brain as the victim node (discard local data)
/// Executes: disconnect -> secondary -> connect --discard-my-data
async fn recover_split_brain(name: &str) -> AppResult<Json<ResourceActionResponse>> {
    info!(
        "Starting split brain recovery for resource {} (this node will discard its data)",
        name
    );

    // Step 1: Disconnect
    let disconnect_cmd = DrbdCmd::disconnect_cmd(name)?;
    let output = run_shell_command(
        &disconnect_cmd,
        &format!("Disconnect DRBD resource {} for split brain recovery", name),
    )
    .await?;
    if !output.success() && !output.stderr.contains("not connected") {
        return Ok(Json(ResourceActionResponse {
            resource: name.to_string(),
            action: "recover_split_brain".to_string(),
            success: false,
            message: Some(format!("Failed to disconnect: {}", output.stderr)),
        }));
    }

    // Step 2: Demote to secondary
    let secondary_cmd = DrbdCmd::secondary_cmd(name)?;
    let output = run_shell_command(
        &secondary_cmd,
        &format!("Demote DRBD resource {} to secondary", name),
    )
    .await?;
    if !output.success() && !output.stderr.contains("already secondary") {
        return Ok(Json(ResourceActionResponse {
            resource: name.to_string(),
            action: "recover_split_brain".to_string(),
            success: false,
            message: Some(format!("Failed to demote to secondary: {}", output.stderr)),
        }));
    }

    // Step 3: Connect with discard-my-data
    let connect_cmd = DrbdCmd::connect_discard_cmd(name)?;
    let output = run_shell_command(
        &connect_cmd,
        &format!("Connect DRBD resource {} with discard-my-data", name),
    )
    .await?;

    Ok(Json(ResourceActionResponse {
        resource: name.to_string(),
        action: "recover_split_brain".to_string(),
        success: output.success(),
        message: if output.success() {
            Some("Split brain recovery initiated. This node will resync from peer.".to_string())
        } else {
            Some(format!(
                "Failed to connect with discard-my-data: {}",
                output.stderr
            ))
        },
    }))
}

/// POST /api/v1/resources/:name/init
/// Initialize resource (create-md) on all nodes
#[utoipa::path(
    post,
    path = "/api/v1/resources/{name}/init",
    tag = "resources",
    params(
        ("name" = String, Path, description = "Resource name")
    ),
    responses(
        (status = 200, description = "Resource initialized", body = ResourceActionResponse),
        (status = 404, description = "Resource not found")
    )
)]
pub async fn init_resource(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> AppResult<Json<ResourceActionResponse>> {
    validator::validate_resource_name(&name)?;
    info!(
        "init_resource: Starting initialization for resource '{}'",
        name
    );

    let operation_id = uuid::Uuid::new_v4().to_string();

    // First, ensure config file exists locally
    let config_path = ConfigPaths::drbd_resource_path(&name);
    info!("init_resource: Checking local config at {}", config_path);

    if !state
        .controller_file_exists(&config_path)
        .await
        .unwrap_or(false)
    {
        info!("init_resource: Local config file not found!");
        state.send_progress(
            &operation_id,
            "init_resource",
            Some(&name),
            0,
            "Config file not found",
            true,
            Some(false),
        );
        return Err(AppError::NotFound(format!(
            "DRBD resource config '{}' not found. Create the resource first.",
            name
        )));
    }
    info!("init_resource: Local config exists");

    // Read local config for syncing to remote nodes
    let config_content = state.read_controller_file(&config_path).await?;
    info!(
        "init_resource: Read local config, {} bytes",
        config_content.len()
    );

    state.send_progress(
        &operation_id,
        "init_resource",
        Some(&name),
        5,
        "Verifying config on all nodes...",
        false,
        None,
    );

    // Get all remote nodes
    let nodes = state.node_store.get_all()?;
    info!("init_resource: Found {} total nodes in store", nodes.len());

    let remote_nodes: Vec<_> = nodes
        .iter()
        .filter(|node| !state.is_controller_node(node))
        .collect();
    info!(
        "init_resource: {} remote nodes: {:?}",
        remote_nodes.len(),
        remote_nodes.iter().map(|n| &n.hostname).collect::<Vec<_>>()
    );

    let total_nodes = remote_nodes.len() + 1;
    let progress_per_node = 30 / total_nodes.max(1);

    // Removed unused creds acquisition

    // Check config exists on all remote nodes, sync only if missing
    for node in &remote_nodes {
        info!(
            "init_resource: Checking node {} ({})",
            node.hostname, node.ip
        );

        let credential = crate::core::SshCredential::Password("ignored".to_string());

        // Check if config exists on remote node
        let check_cmd = format!("test -f '{}' && echo EXISTS", config_path);
        info!(
            "init_resource: Checking if config exists on {}",
            node.hostname
        );
        let check_result = state
            .ssh_manager
            .execute(
                &node.ip,
                node.ssh_port,
                &node.ssh_user,
                &credential,
                &check_cmd,
            )
            .await;

        let config_exists = check_result
            .as_ref()
            .map(|o| o.stdout.contains("EXISTS"))
            .unwrap_or(false);
        info!(
            "init_resource: Config exists on {}: {}",
            node.hostname, config_exists
        );

        if !config_exists {
            // Config missing, sync it
            info!(
                "init_resource: Config MISSING on {}, syncing...",
                node.hostname
            );
            match state
                .ssh_manager
                .write_file(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &config_path,
                    &config_content,
                )
                .await
            {
                Ok(_) => {
                    info!("init_resource: Config synced to {}", node.hostname);
                }
                Err(e) => {
                    tracing::warn!(
                        "init_resource: Failed to sync config to {}: {}",
                        node.hostname,
                        e
                    );
                }
            }
        } else {
            info!("init_resource: Config already exists on {}", node.hostname);
        }
    }

    state.send_progress(
        &operation_id,
        "init_resource",
        Some(&name),
        15,
        "Creating DRBD metadata on local node...",
        false,
        None,
    );

    // Create metadata locally
    info!("init_resource: Creating metadata locally");
    let cmd = DrbdCmd::create_md_cmd(&name)?;
    let output =
        run_shell_command(&cmd, &format!("Create DRBD metadata for resource {}", name)).await?;
    info!(
        "init_resource: Local create-md result: success={}, stderr='{}'",
        output.success(),
        output.stderr
    );

    if !output.success() {
        state.send_progress(
            &operation_id,
            "init_resource",
            Some(&name),
            20,
            &format!("Failed locally: {}", output.stderr),
            true,
            Some(false),
        );
        state.send_notification(
            NotificationLevel::Error,
            "Init Failed",
            &format!(
                "Failed to initialize resource '{}': {}",
                name, output.stderr
            ),
        );
        return Ok(Json(ResourceActionResponse {
            resource: name,
            action: "init".to_string(),
            success: false,
            message: Some(output.stderr),
        }));
    }

    // Create metadata on remote nodes. Track peers that fail so a degraded
    // cluster (peers without metadata / not brought up) is surfaced instead of
    // being silently reported as a fully-created resource.
    let mut peer_warnings: Vec<String> = Vec::new();
    let mut progress = 25;
    // NOTE: no "2>&1 || true" here — that would force exit 0 and discard stderr,
    // making the success check below dead code and hiding real peer failures.
    let create_md_cmd = DrbdCmd::create_md_cmd(&name)?;
    info!("init_resource: Creating metadata on remote nodes");

    for node in &remote_nodes {
        progress += progress_per_node;
        info!("init_resource: Creating metadata on {}", node.hostname);
        state.send_progress(
            &operation_id,
            "init_resource",
            Some(&name),
            progress as u8,
            &format!("Creating metadata on {}...", node.hostname),
            false,
            None,
        );

        // Get credential for remote node
        let credential = crate::core::SshCredential::Password("ignored".to_string());

        match state
            .ssh_manager
            .execute(
                &node.ip,
                node.ssh_port,
                &node.ssh_user,
                &credential,
                &create_md_cmd,
            )
            .await
        {
            Ok(result) => {
                info!(
                    "init_resource: create-md on {} result: success={}, stdout='{}', stderr='{}'",
                    node.hostname,
                    result.success(),
                    result.stdout.trim(),
                    result.stderr.trim()
                );
                if !result.success() && !result.stderr.contains("already initialized") {
                    tracing::warn!(
                        "init_resource: Failed to create metadata on {}: {}",
                        node.hostname,
                        result.stderr
                    );
                    peer_warnings.push(format!(
                        "{} (create-md: {})",
                        node.hostname,
                        result.stderr.trim()
                    ));
                }
            }
            Err(e) => {
                tracing::warn!(
                    "init_resource: Failed to execute on {}: {}",
                    node.hostname,
                    e
                );
                peer_warnings.push(format!("{} (create-md: {})", node.hostname, e));
            }
        }
    }

    state.send_progress(
        &operation_id,
        "init_resource",
        Some(&name),
        50,
        "Bringing up resource on local node...",
        false,
        None,
    );

    // Bring up resource locally
    info!("init_resource: Bringing up resource locally");
    let up_cmd = DrbdCmd::up_cmd(&name)?;
    let up_output = run_shell_command(
        &up_cmd,
        &format!("Bring up DRBD resource {} on local node", name),
    )
    .await?;
    info!(
        "init_resource: Local up result: success={}, stderr='{}'",
        up_output.success(),
        up_output.stderr
    );

    if !up_output.success() {
        state.send_progress(
            &operation_id,
            "init_resource",
            Some(&name),
            60,
            &format!("Failed to bring up locally: {}", up_output.stderr),
            true,
            Some(false),
        );
        state.send_notification(
            NotificationLevel::Error,
            "Init Failed",
            &format!(
                "Failed to bring up resource '{}': {}",
                name, up_output.stderr
            ),
        );
        return Ok(Json(ResourceActionResponse {
            resource: name,
            action: "init".to_string(),
            success: false,
            message: Some(up_output.stderr),
        }));
    }

    // Bring up resource on remote nodes (no "2>&1 || true" — see create-md note).
    info!("init_resource: Bringing up resource on remote nodes");
    let up_remote_cmd = DrbdCmd::up_cmd(&name)?;
    progress = 60;

    for node in &remote_nodes {
        progress += progress_per_node;
        info!("init_resource: Bringing up on {}", node.hostname);
        state.send_progress(
            &operation_id,
            "init_resource",
            Some(&name),
            progress as u8,
            &format!("Bringing up on {}...", node.hostname),
            false,
            None,
        );

        let credential = crate::core::SshCredential::Password("ignored".to_string());

        match state
            .ssh_manager
            .execute(
                &node.ip,
                node.ssh_port,
                &node.ssh_user,
                &credential,
                &up_remote_cmd,
            )
            .await
        {
            Ok(result) => {
                info!(
                    "init_resource: up on {} result: success={}, stdout='{}', stderr='{}'",
                    node.hostname,
                    result.success(),
                    result.stdout.trim(),
                    result.stderr.trim()
                );
                // Re-running `drbdadm up` on an already-configured resource is
                // benign; only treat genuinely failed bring-ups as warnings.
                let benign =
                    result.stderr.contains("already") || result.stderr.contains("is configured");
                if !result.success() && !benign {
                    tracing::warn!(
                        "init_resource: Failed to bring up on {}: {}",
                        node.hostname,
                        result.stderr
                    );
                    peer_warnings.push(format!("{} (up: {})", node.hostname, result.stderr.trim()));
                }
            }
            Err(e) => {
                tracing::warn!(
                    "init_resource: Failed to execute on {}: {}",
                    node.hostname,
                    e
                );
                peer_warnings.push(format!("{} (up: {})", node.hostname, e));
            }
        }
    }

    info!(
        "init_resource: Completed initialization for resource '{}'",
        name
    );
    if peer_warnings.is_empty() {
        state.send_progress(
            &operation_id,
            "init_resource",
            Some(&name),
            100,
            "Resource initialized on all nodes",
            true,
            Some(true),
        );
        state.send_notification(
            NotificationLevel::Success,
            "Resource Initialized",
            &format!("DRBD resource '{}' initialized on all nodes", name),
        );

        Ok(Json(ResourceActionResponse {
            resource: name,
            action: "init".to_string(),
            success: true,
            message: Some("Resource initialized and brought up on all nodes".to_string()),
        }))
    } else {
        // The local node was initialized, but one or more peers failed — the
        // resource exists yet replication is degraded. Surface this loudly.
        let detail = peer_warnings.join("; ");
        state.send_progress(
            &operation_id,
            "init_resource",
            Some(&name),
            100,
            &format!("Initialized locally, but some peers failed: {}", detail),
            true,
            Some(false),
        );
        state.send_notification(
            NotificationLevel::Warning,
            "Resource Initialized With Warnings",
            &format!(
                "DRBD resource '{}' was initialized locally, but these peers failed (replication degraded): {}",
                name, detail
            ),
        );

        Ok(Json(ResourceActionResponse {
            resource: name,
            action: "init".to_string(),
            success: false,
            message: Some(format!("Initialized locally; peers failed: {}", detail)),
        }))
    }
}

/// POST /api/v1/resources/:name/mkfs
/// Create filesystem on DRBD device (must be primary)
#[utoipa::path(
    post,
    path = "/api/v1/resources/{name}/mkfs",
    tag = "resources",
    params(
        ("name" = String, Path, description = "Resource name")
    ),
    request_body = CreateFilesystemRequest,
    responses(
        (status = 200, description = "Filesystem created", body = ResourceActionResponse),
        (status = 400, description = "Invalid request")
    )
)]
pub async fn create_filesystem(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<CreateFilesystemRequest>,
) -> AppResult<Json<ResourceActionResponse>> {
    info!(
        "create_filesystem: Starting for resource '{}', fstype='{}'",
        name, req.fstype
    );
    validator::validate_resource_name(&name)?;

    let operation_id = uuid::Uuid::new_v4().to_string();
    state.send_progress(
        &operation_id,
        "mkfs",
        Some(&name),
        0,
        "Checking resource status...",
        false,
        None,
    );

    // Get device path from resource status
    let status_cmd = DrbdCmd::resource_status_cmd(&name)?;
    info!("create_filesystem: Running '{}'", status_cmd);
    let status_output = run_shell_command(
        &status_cmd,
        &format!("Get DRBD resource status for {}", name),
    )
    .await?;
    info!(
        "create_filesystem: status result: success={}, stdout='{}'",
        status_output.success(),
        status_output.stdout.trim()
    );

    if !status_output.success() {
        info!("create_filesystem: Resource '{}' not found", name);
        return Err(AppError::NotFound(format!("Resource {} not found", name)));
    }

    let resources = parse_drbd_status(&status_output.stdout)?;
    let resource = resources
        .first()
        .ok_or_else(|| AppError::NotFound(format!("Resource {} not found", name)))?;
    info!(
        "create_filesystem: Resource role='{}', is_primary={}",
        resource.role,
        resource.is_primary()
    );

    // Check if resource is primary, if not try to promote
    if !resource.is_primary() {
        info!("create_filesystem: Resource not Primary, need to promote");
        state.send_progress(
            &operation_id,
            "mkfs",
            Some(&name),
            10,
            "Promoting resource to primary...",
            false,
            None,
        );

        // Try normal promote first
        let promote_cmd = DrbdCmd::primary_cmd(&name, false)?;
        info!("create_filesystem: Running '{}'", promote_cmd);
        let promote_output =
            run_shell_command(&promote_cmd, &format!("Promote DRBD resource {}", name)).await?;
        info!(
            "create_filesystem: promote result: success={}, stderr='{}'",
            promote_output.success(),
            promote_output.stderr.trim()
        );

        if !promote_output.success() {
            // If normal promote fails, try force (for new resources with Inconsistent data)
            if promote_output
                .stderr
                .contains("Need access to UpToDate data")
            {
                info!("create_filesystem: Normal promote failed with 'Need access to UpToDate data', trying force promote");
                let force_cmd = DrbdCmd::primary_cmd(&name, true)?;
                info!("create_filesystem: Running '{}'", force_cmd);
                let force_output =
                    run_shell_command(&force_cmd, &format!("Force promote DRBD resource {}", name))
                        .await?;
                info!(
                    "create_filesystem: force promote result: success={}, stderr='{}'",
                    force_output.success(),
                    force_output.stderr.trim()
                );
                if !force_output.success() {
                    info!(
                        "create_filesystem: Force promote failed: {}",
                        force_output.stderr
                    );
                    state.send_progress(
                        &operation_id,
                        "mkfs",
                        Some(&name),
                        15,
                        &format!("Failed to promote: {}", force_output.stderr),
                        true,
                        Some(false),
                    );
                    return Err(AppError::Drbd(format!(
                        "Failed to promote resource to primary: {}",
                        force_output.stderr
                    )));
                }
                info!("create_filesystem: Force promote succeeded");
            } else {
                info!(
                    "create_filesystem: Promote failed: {}",
                    promote_output.stderr
                );
                state.send_progress(
                    &operation_id,
                    "mkfs",
                    Some(&name),
                    15,
                    &format!("Failed to promote: {}", promote_output.stderr),
                    true,
                    Some(false),
                );
                return Err(AppError::Drbd(format!(
                    "Failed to promote resource to primary: {}",
                    promote_output.stderr
                )));
            }
        } else {
            info!("create_filesystem: Normal promote succeeded");
        }

        // Wait a bit for promotion to complete
        info!("create_filesystem: Waiting 500ms for promotion to complete");
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // Get device path (e.g., /dev/drbd0)
    let device = resource
        .devices
        .first()
        .map(|d| format!("/dev/drbd{}", d.minor))
        .ok_or_else(|| AppError::Drbd("No device found for resource".to_string()))?;
    info!("create_filesystem: Device path: {}", device);

    state.send_progress(
        &operation_id,
        "mkfs",
        Some(&name),
        20,
        "Running safety checks...",
        false,
        None,
    );

    // Safety check: Verify DRBD device is safe for mkfs
    // This prevents accidentally formatting the wrong device
    let safety_checker = SafetyChecker::new(state.ssh_manager.clone());
    info!(
        "create_filesystem: Running safety checks before mkfs on {}",
        device
    );

    let check = safety_checker.check_device_for_mkfs(&device).await?;
    info!(
        "create_filesystem: Safety check result: errors={:?}, warnings={:?}",
        check.errors, check.warnings
    );
    if let Some(err) = check.to_error() {
        info!("create_filesystem: Safety check failed: {}", err);
        state.send_progress(
            &operation_id,
            "mkfs",
            Some(&name),
            20,
            &format!("Safety check failed: {}", err),
            true,
            Some(false),
        );
        return Err(err);
    }

    // Log warnings but proceed
    for warning in &check.warnings {
        tracing::warn!("create_filesystem: Device {}: {}", device, warning);
    }

    state.send_progress(
        &operation_id,
        "mkfs",
        Some(&name),
        40,
        &format!("Creating {} filesystem on {}...", req.fstype, device),
        false,
        None,
    );
    info!(
        "create_filesystem: Safety checks passed, creating {} filesystem on {}",
        req.fstype, device
    );

    // Create filesystem
    let mkfs_cmd = DrbdCmd::mkfs_cmd(&device, &req.fstype)?;
    info!("create_filesystem: Running '{}'", mkfs_cmd);
    let output = run_shell_command(
        &mkfs_cmd,
        &format!("Create {} filesystem on {}", req.fstype, device),
    )
    .await?;
    info!(
        "create_filesystem: mkfs result: success={}, stdout='{}', stderr='{}'",
        output.success(),
        output.stdout.trim(),
        output.stderr.trim()
    );

    if output.success() {
        info!("create_filesystem: Filesystem created successfully");
        state.send_progress(
            &operation_id,
            "mkfs",
            Some(&name),
            100,
            &format!("Created {} filesystem on {}", req.fstype, device),
            true,
            Some(true),
        );
        state.send_notification(
            NotificationLevel::Success,
            "Filesystem Created",
            &format!("Created {} filesystem on resource '{}'", req.fstype, name),
        );
    } else {
        info!("create_filesystem: mkfs failed: {}", output.stderr);
        state.send_progress(
            &operation_id,
            "mkfs",
            Some(&name),
            100,
            &format!("Failed: {}", output.stderr),
            true,
            Some(false),
        );
        state.send_notification(
            NotificationLevel::Error,
            "Mkfs Failed",
            &format!(
                "Failed to create filesystem on '{}': {}",
                name, output.stderr
            ),
        );
    }

    Ok(Json(ResourceActionResponse {
        resource: name,
        action: format!("mkfs.{}", req.fstype),
        success: output.success(),
        message: if output.success() {
            Some(format!("Created {} filesystem on {}", req.fstype, device))
        } else {
            Some(output.stderr)
        },
    }))
}

/// POST /api/v1/resources/:name/mount
/// Mount DRBD device (must be primary)
#[utoipa::path(
    post,
    path = "/api/v1/resources/{name}/mount",
    tag = "resources",
    params(
        ("name" = String, Path, description = "Resource name")
    ),
    request_body = MountRequest,
    responses(
        (status = 200, description = "Device mounted", body = ResourceActionResponse),
        (status = 400, description = "Invalid request")
    )
)]
pub async fn mount_resource(
    Path(name): Path<String>,
    Json(req): Json<MountRequest>,
) -> AppResult<Json<ResourceActionResponse>> {
    info!(
        "mount_resource: Starting for resource '{}', mount_point='{}'",
        name, req.mount_point
    );
    validator::validate_resource_name(&name)?;
    validator::validate_mount_point(&req.mount_point)?;

    // Get device path from resource status
    let status_cmd = DrbdCmd::resource_status_cmd(&name)?;
    info!("mount_resource: Running '{}'", status_cmd);
    let status_output = run_shell_command(
        &status_cmd,
        &format!("Get DRBD resource status for {}", name),
    )
    .await?;
    info!(
        "mount_resource: status result: success={}, stdout='{}'",
        status_output.success(),
        status_output.stdout.trim()
    );

    if !status_output.success() {
        info!("mount_resource: Resource '{}' not found", name);
        return Err(AppError::NotFound(format!("Resource {} not found", name)));
    }

    let resources = parse_drbd_status(&status_output.stdout)?;
    let resource = resources
        .first()
        .ok_or_else(|| AppError::NotFound(format!("Resource {} not found", name)))?;
    info!(
        "mount_resource: Resource role='{}', is_primary={}",
        resource.role,
        resource.is_primary()
    );

    if !resource.is_primary() {
        info!(
            "mount_resource: Resource must be Primary to mount, currently '{}'",
            resource.role
        );
        return Err(AppError::Drbd(
            "Resource must be primary to mount".to_string(),
        ));
    }

    let device = resource
        .devices
        .first()
        .map(|d| format!("/dev/drbd{}", d.minor))
        .ok_or_else(|| AppError::Drbd("No device found for resource".to_string()))?;
    info!("mount_resource: Device path: {}", device);

    // Create mount point directory
    let mkdir_cmd = DrbdCmd::mkdir_cmd(&req.mount_point)?;
    info!("mount_resource: Running '{}'", mkdir_cmd);
    let mkdir_output = run_shell_command(
        &mkdir_cmd,
        &format!("Create mount point directory {}", req.mount_point),
    )
    .await?;
    info!(
        "mount_resource: mkdir result: success={}, stderr='{}'",
        mkdir_output.success(),
        mkdir_output.stderr.trim()
    );

    // Mount device
    let mount_cmd = DrbdCmd::mount_cmd(&device, &req.mount_point)?;
    info!("mount_resource: Running '{}'", mount_cmd);
    let output = run_shell_command(
        &mount_cmd,
        &format!("Mount DRBD device {} to {}", device, req.mount_point),
    )
    .await?;
    info!(
        "mount_resource: mount result: success={}, stderr='{}'",
        output.success(),
        output.stderr.trim()
    );

    Ok(Json(ResourceActionResponse {
        resource: name,
        action: "mount".to_string(),
        success: output.success(),
        message: if output.success() {
            info!("mount_resource: Mount succeeded");
            Some(format!("Mounted {} at {}", device, req.mount_point))
        } else {
            info!("mount_resource: Mount failed: {}", output.stderr);
            Some(output.stderr)
        },
    }))
}

/// POST /api/v1/resources/:name/umount
/// Unmount DRBD device
#[utoipa::path(
    post,
    path = "/api/v1/resources/{name}/umount",
    tag = "resources",
    params(
        ("name" = String, Path, description = "Resource name")
    ),
    request_body = MountRequest,
    responses(
        (status = 200, description = "Device unmounted", body = ResourceActionResponse),
        (status = 400, description = "Invalid request")
    )
)]
pub async fn umount_resource(
    Path(name): Path<String>,
    Json(req): Json<MountRequest>,
) -> AppResult<Json<ResourceActionResponse>> {
    validator::validate_resource_name(&name)?;
    validator::validate_mount_point(&req.mount_point)?;

    let umount_cmd = DrbdCmd::umount_cmd(&req.mount_point)?;
    let output = run_shell_command(&umount_cmd, &format!("Unmount {}", req.mount_point)).await?;

    Ok(Json(ResourceActionResponse {
        resource: name,
        action: "umount".to_string(),
        success: output.success(),
        message: if output.success() {
            Some(format!("Unmounted {}", req.mount_point))
        } else {
            Some(output.stderr)
        },
    }))
}

/// Query parameters for log retrieval
#[derive(Debug, serde::Deserialize, ToSchema)]
pub struct LogsQuery {
    /// Number of lines to retrieve (default: 100, max: 1000)
    #[serde(default = "default_log_lines")]
    pub lines: u32,
    /// Filter logs since this time (e.g., "1h", "30m", "2024-01-15")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}

fn default_log_lines() -> u32 {
    100
}

/// Response for log retrieval
#[derive(Serialize, ToSchema)]
pub struct LogsResponse {
    pub resource: String,
    pub lines: Vec<String>,
    pub total_lines: usize,
}

/// GET /api/v1/resources/:name/logs
/// Get DRBD resource logs from journalctl
#[utoipa::path(
    get,
    path = "/api/v1/resources/{name}/logs",
    tag = "resources",
    params(
        ("name" = String, Path, description = "Resource name"),
        ("lines" = Option<u32>, Query, description = "Number of lines to retrieve (default: 100, max: 1000)"),
        ("since" = Option<String>, Query, description = "Filter logs since this time (e.g., '1h', '30m')")
    ),
    responses(
        (status = 200, description = "Resource logs", body = LogsResponse)
    )
)]
pub async fn get_resource_logs(
    Path(name): Path<String>,
    axum::extract::Query(query): axum::extract::Query<LogsQuery>,
) -> AppResult<Json<LogsResponse>> {
    validator::validate_resource_name(&name)?;

    // Limit lines to prevent excessive output
    let lines = query.lines.min(1000);

    // Build journalctl command for DRBD promoter service
    let mut cmd = format!(
        "journalctl -u 'drbd-promote@{}.service' -n {} --no-pager",
        name, lines
    );

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

    let output = run_shell_command(
        &cmd,
        &format!("Get journalctl logs for drbd-promote@{} service", name),
    )
    .await?;

    let log_lines: Vec<String> = output.stdout.lines().map(|s| s.to_string()).collect();

    Ok(Json(LogsResponse {
        resource: name,
        total_lines: log_lines.len(),
        lines: log_lines,
    }))
}

/// DELETE /api/v1/resources/:name
/// Delete a DRBD resource
#[utoipa::path(
    delete,
    path = "/api/v1/resources/{name}",
    tag = "resources",
    params(
        ("name" = String, Path, description = "Resource name")
    ),
    responses(
        (status = 204, description = "Resource deleted"),
        (status = 404, description = "Resource not found")
    )
)]
pub async fn delete_resource(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> AppResult<StatusCode> {
    validator::validate_resource_name(&name)?;

    // First bring down the resource
    let down_cmd = DrbdCmd::down_cmd(&name)?;
    let _ = run_shell_command(&down_cmd, &format!("Bring down DRBD resource {}", name)).await;

    // Delete configuration file locally
    let config_path = ConfigPaths::drbd_resource_path(&name);
    if state
        .controller_file_exists(&config_path)
        .await
        .unwrap_or(false)
    {
        state.remove_controller_file(&config_path).await?;
    }

    // Delete configuration file from all remote nodes
    if let Ok(nodes) = state.node_store.get_all() {
        let remote_nodes: Vec<_> = nodes
            .iter()
            .filter(|node| !state.is_controller_node(node))
            .collect();

        for node in remote_nodes {
            // Get credential (dummy for now)
            let credential = crate::core::SshCredential::Password("ignored".to_string());

            let rm_cmd = format!("rm -f '{}'", config_path);
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

    Ok(StatusCode::NO_CONTENT)
}

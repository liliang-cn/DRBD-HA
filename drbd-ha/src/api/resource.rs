//! DRBD resource management API handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use tracing::info;

use crate::core::{
    config_gen::{ConfigGenerator, ConfigPaths, NodeConfig, ResourceConfig},
    drbd_cmd::{parse_drbd_status, DrbdCmd, ResourceStatus},
    run_shell_command,
    safety::SafetyChecker,
    transaction::NodeTarget,
    validator,
};
use crate::error::{AppError, AppResult};
use crate::models::{
    CreateFilesystemRequest, CreateResourceRequest, MountRequest, ResourceAction,
    ResourceActionRequest,
};
use crate::state::{AppState, NotificationLevel};

/// Response for resource list
#[derive(Serialize)]
pub struct ResourceListResponse {
    pub resources: Vec<ResourceStatus>,
}

/// Response for resource creation
#[derive(Serialize)]
pub struct ResourceCreateResponse {
    pub name: String,
    pub message: String,
    pub config_path: String,
}

/// Response for resource action
#[derive(Serialize)]
pub struct ResourceActionResponse {
    pub resource: String,
    pub action: String,
    pub success: bool,
    pub message: Option<String>,
}

/// GET /api/v1/resources
/// List all DRBD resources
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
    validator::validate_minor(req.minor)?;

    if req.node_disks.is_empty() {
        return Err(AppError::Validation(
            "At least one node disk must be specified".to_string(),
        ));
    }

    // Validate all block devices
    for disk in req.node_disks.values() {
        validator::validate_block_device(disk)?;
    }

    // Get node information from database
    let creds = state.credentials.read().await;

    // Build node configs and collect targets for safety checks
    let mut node_configs: Vec<NodeConfig> = Vec::new();
    let mut targets: Vec<NodeTarget> = Vec::new();
    let mut local_disks: Vec<String> = Vec::new();
    let mut remote_checks: Vec<(String, u16, String, crate::core::SshCredential, String)> =
        Vec::new();

    for (node_id, disk) in &req.node_disks {
        let node = state
            .db
            .get_node(node_id)?
            .ok_or_else(|| AppError::NotFound(format!("Node {} not found", node_id)))?;

        node_configs.push(NodeConfig {
            hostname: node.hostname.clone(),
            ip: node.ip.clone(),
            disk: disk.clone(),
            node_id: node_configs.len() as u32,
        });

        // Get credential for remote nodes
        if !node.is_local {
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
                disk.clone(),
            ));
        } else {
            local_disks.push(disk.clone());
        }
    }

    drop(creds);

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
    let resource_config = ResourceConfig {
        name: req.name.clone(),
        port: req.port,
        minor: req.minor,
        device: format!("/dev/drbd{}", req.minor),
        nodes: node_configs,
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
    tokio::fs::write(&config_path, &config_content)
        .await
        .map_err(|e| AppError::Config(format!("Failed to write config: {}", e)))?;

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
    let all_nodes = state.db.get_all_nodes()?;
    info!(
        "create_resource: Found {} total nodes in database",
        all_nodes.len()
    );

    let remote_nodes: Vec<_> = all_nodes.iter().filter(|n| !n.is_local).collect();
    info!(
        "create_resource: {} remote nodes to sync: {:?}",
        remote_nodes.len(),
        remote_nodes.iter().map(|n| &n.hostname).collect::<Vec<_>>()
    );

    let mut written_hosts: Vec<(String, u16, String, crate::core::SshCredential)> = Vec::new();

    // Re-acquire credentials for syncing
    let creds = state.credentials.read().await;
    info!(
        "create_resource: Credentials available for {} nodes",
        creds.len()
    );

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
                let _ = tokio::fs::remove_file(&config_path).await;
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

    if tokio::fs::metadata(&config_path).await.is_err() {
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
    let config_content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|e| AppError::Config(format!("Failed to read config: {}", e)))?;
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
    let nodes = state.db.get_all_nodes()?;
    info!(
        "init_resource: Found {} total nodes in database",
        nodes.len()
    );

    let remote_nodes: Vec<_> = nodes.iter().filter(|n| !n.is_local).collect();
    info!(
        "init_resource: {} remote nodes: {:?}",
        remote_nodes.len(),
        remote_nodes.iter().map(|n| &n.hostname).collect::<Vec<_>>()
    );

    let total_nodes = remote_nodes.len() + 1;
    let progress_per_node = 30 / total_nodes.max(1);

    let creds = state.credentials.read().await;
    info!(
        "init_resource: Credentials available for {} nodes",
        creds.len()
    );

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

    // Create metadata on remote nodes
    let mut progress = 25;
    let create_md_cmd = format!("drbdadm create-md --force {} 2>&1 || true", name);
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
                }
            }
            Err(e) => {
                tracing::warn!(
                    "init_resource: Failed to execute on {}: {}",
                    node.hostname,
                    e
                );
            }
        }
    }
    drop(creds);

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

    // Bring up resource on remote nodes
    info!("init_resource: Bringing up resource on remote nodes");
    let up_remote_cmd = format!("drbdadm up {} 2>&1 || true", name);
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
                if !result.success() {
                    tracing::warn!(
                        "init_resource: Failed to bring up on {}: {}",
                        node.hostname,
                        result.stderr
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    "init_resource: Failed to execute on {}: {}",
                    node.hostname,
                    e
                );
            }
        }
    }

    info!(
        "init_resource: Completed initialization for resource '{}'",
        name
    );
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
}

/// POST /api/v1/resources/:name/mkfs
/// Create filesystem on DRBD device (must be primary)
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
        let promote_cmd = format!("drbdadm primary {}", name);
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
                let force_cmd = format!("drbdadm primary --force {}", name);
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
#[derive(Debug, serde::Deserialize)]
pub struct LogsQuery {
    /// Number of lines to retrieve (default: 100, max: 1000)
    #[serde(default = "default_log_lines")]
    pub lines: u32,
    /// Filter logs since this time (e.g., "1h", "30m", "2024-01-15")
    pub since: Option<String>,
}

fn default_log_lines() -> u32 {
    100
}

/// Response for log retrieval
#[derive(Serialize)]
pub struct LogsResponse {
    pub resource: String,
    pub lines: Vec<String>,
    pub total_lines: usize,
}

/// GET /api/v1/resources/:name/logs
/// Get DRBD resource logs from journalctl
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
pub async fn delete_resource(
    State(_state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> AppResult<StatusCode> {
    validator::validate_resource_name(&name)?;

    // First bring down the resource
    let down_cmd = DrbdCmd::down_cmd(&name)?;
    let _ = run_shell_command(&down_cmd, &format!("Bring down DRBD resource {}", name)).await;

    // Delete configuration file
    let config_path = ConfigPaths::drbd_resource_path(&name);
    if tokio::fs::metadata(&config_path).await.is_ok() {
        tokio::fs::remove_file(&config_path)
            .await
            .map_err(|e| AppError::Config(format!("Failed to delete config: {}", e)))?;
    }

    Ok(StatusCode::NO_CONTENT)
}

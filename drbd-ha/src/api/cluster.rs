//! Cluster and node management API handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::detected_controller_platform;
use crate::core::{run_shell_command, SshCredential};
use crate::error::{AppError, AppResult};
use crate::models::{AddNodeRequest, BlockDevice, LsblkOutput, Node, NodeStatus};
use crate::state::AppState;

/// Health check response
#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub platform: String,
    pub controller_mode: String,
}

/// GET /api/v1/health
#[utoipa::path(
    get,
    path = "/api/v1/health",
    tag = "cluster",
    responses(
        (status = 200, description = "System is healthy", body = HealthResponse)
    )
)]
pub async fn health_check(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: detected_controller_platform().to_string(),
        controller_mode: state.config.controller.mode.as_str().to_string(),
    })
}

/// Helper to get a node by ID, resolving "local" to the actual local node
fn get_node_by_id_resolved(state: &AppState, id: &str) -> AppResult<Node> {
    if id == "local" {
        let nodes = state.node_store.get_all()?;
        return nodes
            .into_iter()
            .find(|n| state.is_controller_node(n))
            .ok_or_else(|| AppError::NotFound("Local node not found".to_string()));
    }

    state
        .node_store
        .get(id)?
        .ok_or_else(|| AppError::NotFound(format!("Node {} not found", id)))
}

/// Verify remote access requirements for a managed node.
///
/// A node is only considered online if:
/// 1. Passwordless SSH works
/// 2. For non-root users, passwordless sudo (`sudo -n`) also works
async fn verify_remote_node_access(
    state: &AppState,
    ip: &str,
    ssh_port: u16,
    ssh_user: &str,
) -> (NodeStatus, Option<String>) {
    let credential = SshCredential::Password("ignored".to_string());

    let ssh_check = state
        .ssh_manager
        .execute(ip, ssh_port, ssh_user, &credential, "echo ok")
        .await;

    match ssh_check {
        Ok(output) if output.success() => {
            if ssh_user == "root" {
                (NodeStatus::Online, None)
            } else {
                match state
                    .ssh_manager
                    .execute(ip, ssh_port, ssh_user, &credential, "sudo -n true")
                    .await
                {
                    Ok(output) if output.success() => (NodeStatus::Online, None),
                    Ok(output) => (
                        NodeStatus::Error,
                        Some(format!(
                            "SSH ok, but passwordless sudo failed for user '{}': {}",
                            ssh_user,
                            output.stderr.trim()
                        )),
                    ),
                    Err(e) => (
                        NodeStatus::Error,
                        Some(format!(
                            "SSH ok, but sudo validation failed for user '{}': {}",
                            ssh_user, e
                        )),
                    ),
                }
            }
        }
        Ok(output) => (
            NodeStatus::Error,
            Some(format!("SSH check failed: {}", output.stderr.trim())),
        ),
        Err(e) => (
            NodeStatus::Offline,
            Some(format!("SSH connection failed: {}", e)),
        ),
    }
}

fn resolved_ssh_user(config_default: &str, requested: Option<&str>) -> String {
    requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(config_default)
        .to_string()
}

/// Parse node information from DRBD .res configuration files
/// Extracts hostname and IP from "on <hostname> { address ... }" blocks
async fn import_nodes_from_drbd_configs(state: &Arc<AppState>) -> AppResult<Vec<Node>> {
    use std::collections::HashMap;

    let config_path = &state.config.drbd.config_path;
    let mut discovered_nodes: HashMap<String, Node> = HashMap::new();

    let controller_hostname = state.controller_hostname();

    // Read all .res files
    let entries = match state.list_controller_dir_entries(config_path).await {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("Cannot read DRBD config directory {}: {}", config_path, e);
            return Ok(Vec::new());
        }
    };

    for file_name in entries {

        if !file_name.ends_with(".res") {
            continue;
        }

        // Read .res file content
        let file_path = format!("{}/{}", config_path.trim_end_matches('/'), file_name);
        let content = match state.read_controller_file(&file_path).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to read {}: {}", file_name, e);
                continue;
            }
        };

        // Parse "on <hostname> { address <ip>:<port>; }" blocks
        // Use a simple state machine to find these blocks
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].trim();

            // Look for "on <hostname> {" pattern
            if line.starts_with("on ") && line.ends_with('{') {
                let hostname_part = line[2..line.len() - 1].trim();
                let hostname = hostname_part.to_string();

                // Look for address line within the next few lines
                let mut address = None;
                let mut j = i + 1;
                while j < lines.len() && j < i + 20 {
                    let inner_line = lines[j].trim();

                    // Exit the block when we see closing brace
                    if inner_line == "}" {
                        break;
                    }

                    // Parse address line: address    <ip>:<port>;
                    if inner_line.starts_with("address ") {
                        let addr_part = inner_line[8..].trim().trim_end_matches(';');
                        // Parse IP:port, we only need the IP
                        if let Some(colon_pos) = addr_part.find(':') {
                            let ip = addr_part[..colon_pos].trim().to_string();
                            address = Some(ip);
                        }
                        break;
                    }

                    j += 1;
                }

                if let Some(ip) = address {
                    let is_local = if state.is_external_controller() {
                        state.matches_controller_target(&hostname, &ip, &hostname)
                    } else {
                        hostname == controller_hostname
                    };

                    // Use existing SSH settings if node already exists
                    let (ssh_port, ssh_user, status, status_message, last_seen) =
                        if let Ok(Some(existing)) = state.node_store.get(&hostname) {
                            (
                                existing.ssh_port,
                                existing.ssh_user,
                                existing.status,
                                existing.status_message,
                                existing.last_seen,
                            )
                        } else {
                            let ssh_port = state.config.ssh.default_port;
                            let ssh_user = state.config.ssh.default_user.clone();

                            // Try to check connectivity and sudo capability
                            let (status, status_message) =
                                verify_remote_node_access(state, &ip, ssh_port, &ssh_user).await;
                            let last_seen = if status == NodeStatus::Online {
                                Some(chrono::Utc::now())
                            } else {
                                None
                            };

                            (ssh_port, ssh_user, status, status_message, last_seen)
                        };

                    let node = Node {
                        id: hostname.clone(),
                        hostname: hostname.clone(),
                        ip,
                        ssh_port,
                        ssh_user,
                        is_local,
                        status,
                        status_message,
                        last_seen,
                    };

                    discovered_nodes.insert(hostname, node);
                }
            }

            i += 1;
        }
    }

    Ok(discovered_nodes.into_values().collect())
}

/// Sync nodes from DRBD configs with the node store
/// Returns all nodes (both from configs and manually added)
pub async fn sync_nodes_from_drbd(state: &Arc<AppState>) -> AppResult<Vec<Node>> {
    // Import nodes from .res files
    let discovered = import_nodes_from_drbd_configs(state).await?;

    // Get existing manually added nodes
    let existing = state.node_store.get_all().unwrap_or_default();

    // Merge: nodes from .res files + existing nodes that aren't in .res files
    use std::collections::HashSet;
    let discovered_hostnames: HashSet<String> =
        discovered.iter().map(|n| n.hostname.clone()).collect();

    let mut all_nodes = discovered;
    for node in existing {
        if !discovered_hostnames.contains(node.hostname.as_str()) {
            all_nodes.push(node);
        }
    }

    // Update the store with merged nodes
    for node in &all_nodes {
        let _ = state.node_store.insert(node);
    }

    state.refresh_command_proxy();

    Ok(all_nodes)
}

/// GET /api/v1/nodes
#[utoipa::path(
    get,
    path = "/api/v1/nodes",
    tag = "cluster",
    responses(
        (status = 200, description = "List all nodes", body = [Node])
    )
)]
pub async fn list_nodes(State(state): State<Arc<AppState>>) -> AppResult<Json<Vec<Node>>> {
    // Sync nodes from DRBD configs first, then return all nodes
    let nodes = sync_nodes_from_drbd(&state).await?;
    Ok(Json(nodes))
}

/// POST /api/v1/nodes
#[utoipa::path(
    post,
    path = "/api/v1/nodes",
    tag = "cluster",
    request_body = AddNodeRequest,
    responses(
        (status = 201, description = "Node added successfully", body = Node),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Node already exists")
    )
)]
pub async fn add_node(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddNodeRequest>,
) -> AppResult<(StatusCode, Json<Node>)> {
    // Validate request
    if req.hostname.is_empty() || req.ip.is_empty() {
        return Err(AppError::Validation(
            "hostname and ip are required".to_string(),
        ));
    }

    // Check for duplicate
    let existing_nodes = state.node_store.get_all()?;
    if existing_nodes
        .iter()
        .any(|n| n.ip == req.ip || n.hostname == req.hostname)
    {
        return Err(AppError::AlreadyExists(format!(
            "Node with ip {} or hostname {} already exists",
            req.ip, req.hostname
        )));
    }

    // Optional connectivity check; never block add by status.
    let ssh_port = req.ssh_port.unwrap_or(state.config.ssh.default_port);
    let ssh_user = resolved_ssh_user(&state.config.ssh.default_user, req.ssh_user.as_deref());

    let (status, status_message) =
        verify_remote_node_access(&state, &req.ip, ssh_port, &ssh_user).await;
    let is_local = state.matches_controller_target(&req.hostname, &req.ip, &req.hostname);

    // Create node
    let node = Node {
        id: uuid::Uuid::new_v4().to_string(),
        hostname: req.hostname,
        ip: req.ip,
        ssh_port: req.ssh_port.unwrap_or(state.config.ssh.default_port),
        ssh_user,
        is_local,
        status: status.clone(),
        status_message,
        last_seen: if status == NodeStatus::Online {
            Some(chrono::Utc::now())
        } else {
            None
        },
    };

    // Store node
    state.node_store.insert(&node)?;
    state.refresh_command_proxy();

    Ok((StatusCode::CREATED, Json(node)))
}

/// GET /api/v1/nodes/:id
#[utoipa::path(
    get,
    path = "/api/v1/nodes/{id}",
    tag = "cluster",
    params(
        ("id" = String, Path, description = "Node ID")
    ),
    responses(
        (status = 200, description = "Node details", body = Node),
        (status = 404, description = "Node not found")
    )
)]
pub async fn get_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<Node>> {
    let node = get_node_by_id_resolved(&state, &id)?;
    Ok(Json(node))
}

/// PUT /api/v1/nodes/:id
#[utoipa::path(
    put,
    path = "/api/v1/nodes/{id}",
    tag = "cluster",
    params(
        ("id" = String, Path, description = "Node ID")
    ),
    request_body = AddNodeRequest,
    responses(
        (status = 200, description = "Node updated", body = Node),
        (status = 404, description = "Node not found"),
        (status = 400, description = "Invalid request")
    )
)]
pub async fn update_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AddNodeRequest>,
) -> AppResult<Json<Node>> {
    // Get existing node
    let existing = state
        .node_store
        .get(&id)?
        .ok_or_else(|| AppError::NotFound(format!("Node {} not found", id)))?;

    // Check if new ip/hostname conflicts with another node
    // Exclude: (1) the node itself by id, (2) nodes with same hostname (may be same node from DRBD import)
    let existing_nodes = state.node_store.get_all()?;
    if existing_nodes.iter().any(|n| {
        n.id != id
            && n.hostname != existing.hostname
            && (n.ip == req.ip || n.hostname == req.hostname)
    }) {
        return Err(AppError::AlreadyExists(format!(
            "Node with ip {} or hostname {} already exists",
            req.ip, req.hostname
        )));
    }

    // Update node with new values
    let ssh_port = req.ssh_port.unwrap_or(state.config.ssh.default_port);
    let ssh_user = resolved_ssh_user(&state.config.ssh.default_user, req.ssh_user.as_deref());
    let is_local = state.matches_controller_target(&req.hostname, &req.ip, &existing.id);

    let updated_node = Node {
        id,
        hostname: req.hostname,
        ip: req.ip,
        ssh_port,
        ssh_user,
        is_local,
        status: existing.status,
        status_message: existing.status_message,
        last_seen: existing.last_seen,
    };

    // Update in store
    state.node_store.update(&updated_node)?;
    state.refresh_command_proxy();

    Ok(Json(updated_node))
}

/// DELETE /api/v1/nodes/:id
#[utoipa::path(
    delete,
    path = "/api/v1/nodes/{id}",
    tag = "cluster",
    params(
        ("id" = String, Path, description = "Node ID")
    ),
    responses(
        (status = 204, description = "Node deleted"),
        (status = 404, description = "Node not found")
    )
)]
pub async fn delete_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    if state.node_store.delete(&id)? {
        state.refresh_command_proxy();
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound(format!("Node {} not found", id)))
    }
}

/// GET /api/v1/nodes/:id/disks
#[utoipa::path(
    get,
    path = "/api/v1/nodes/{id}/disks",
    tag = "cluster",
    params(
        ("id" = String, Path, description = "Node ID")
    ),
    responses(
        (status = 200, description = "List of block devices", body = [BlockDevice]),
        (status = 404, description = "Node not found")
    )
)]
pub async fn list_node_disks(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<BlockDevice>>> {
    let node = get_node_by_id_resolved(&state, &id)?;

    // Get block devices using lsblk (add sudo for non-root users)
    let is_controller_node = state.is_controller_node(&node);
    let lsblk_cmd = if node.ssh_user != "root" && !is_controller_node {
        "sudo lsblk -J -b -o NAME,SIZE,TYPE,MOUNTPOINT,FSTYPE,RO,MODEL,PATH"
    } else {
        "lsblk -J -b -o NAME,SIZE,TYPE,MOUNTPOINT,FSTYPE,RO,MODEL,PATH"
    };

    let output: LsblkOutput = if is_controller_node {
        // Local node
        let local_output = run_shell_command(lsblk_cmd, "List block devices locally")
            .await?
            .stdout;
        serde_json::from_str(&local_output)
            .map_err(|e| AppError::Internal(format!("Failed to parse lsblk output: {}", e)))?
    } else {
        // Remote node - use dummy credential
        let credential = SshCredential::Password("ignored".to_string());

        tracing::info!(
            "Attempting SSH to {}@{}:{} to list disks",
            node.ssh_user,
            node.ip,
            node.ssh_port
        );

        match state
            .ssh_manager
            .execute_json(
                &node.ip,
                node.ssh_port,
                &node.ssh_user,
                &credential,
                lsblk_cmd,
            )
            .await
        {
            Ok(output) => output,
            Err(e) => {
                tracing::error!(
                    "SSH command failed for node {} ({}@{}:{}): {}",
                    id,
                    node.ssh_user,
                    node.ip,
                    node.ssh_port,
                    e
                );
                return Err(e);
            }
        }
    };

    // Add human-readable size to all devices
    let mut devices = output.blockdevices;
    add_human_readable_sizes(&mut devices);

    Ok(Json(devices))
}

/// Recursively add human-readable size to devices
fn add_human_readable_sizes(devices: &mut [BlockDevice]) {
    for device in devices.iter_mut() {
        device.size_human = Some(device.size_human());
        add_human_readable_sizes(&mut device.children);
    }
}

/// GET /api/v1/nodes/:id/disks/available
#[utoipa::path(
    get,
    path = "/api/v1/nodes/{id}/disks/available",
    tag = "cluster",
    params(
        ("id" = String, Path, description = "Node ID")
    ),
    responses(
        (status = 200, description = "List of available block devices", body = [BlockDevice]),
        (status = 404, description = "Node not found")
    )
)]
pub async fn list_available_disks(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<BlockDevice>>> {
    let node = get_node_by_id_resolved(&state, &id)?;

    let all_disks_res = list_node_disks(State(state.clone()), Path(id.clone())).await?;

    // Filter to only available disks from lsblk
    let mut available: Vec<BlockDevice> = all_disks_res
        .0
        .into_iter()
        .filter(|d| d.is_available())
        .collect();

    // Setup LVM client (local or remote)
    let lvm_client = if state.is_controller_node(&node) {
        crate::core::lvm_utils::LvmClient::new_local()
    } else {
        // Dummy credential, as in other parts of this file
        let credential = SshCredential::Password("ignored".to_string());
        crate::core::lvm_utils::LvmClient::new_remote(
            state.ssh_manager.clone(),
            node.ip.clone(),
            node.ssh_port,
            node.ssh_user.clone(),
            credential,
        )
    };

    // Add unused LVs
    if let Ok(lvs) = lvm_client.list_available_lvs().await {
        for lv in lvs {
            let path = format!("/dev/{}/{}", lv.vg_name, lv.name);

            // Check if it's already in the list
            if !available.iter().any(|d| d.path.as_ref() == Some(&path)) {
                let mut bd = BlockDevice {
                    name: lv.name,
                    path: Some(path),
                    size: lv.size,
                    size_human: None,
                    device_type: "lvm".to_string(),
                    mountpoint: None,
                    fstype: None,
                    ro: lv.attr.chars().nth(1) == Some('r'),
                    model: None,
                    children: vec![],
                };
                bd.size_human = Some(bd.size_human());
                available.push(bd);
            }
        }
    }

    Ok(Json(available))
}
/// Response for node status check
#[derive(Serialize, ToSchema)]
pub struct NodeStatusResponse {
    pub id: String,
    pub hostname: String,
    pub status: NodeStatus,
    pub message: Option<String>,
}

/// POST /api/v1/nodes/:id/check
#[utoipa::path(
    post,
    path = "/api/v1/nodes/{id}/check",
    tag = "cluster",
    params(
        ("id" = String, Path, description = "Node ID")
    ),
    responses(
        (status = 200, description = "Node status checked", body = NodeStatusResponse),
        (status = 404, description = "Node not found")
    )
)]
pub async fn check_node_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<NodeStatusResponse>> {
    let mut node = get_node_by_id_resolved(&state, &id)?;

    if state.is_controller_node(&node) {
        node.status = NodeStatus::Online;
        node.status_message = None;
        node.last_seen = Some(chrono::Utc::now());
        state.node_store.insert(&node)?;
        state.refresh_command_proxy();

        return Ok(Json(NodeStatusResponse {
            id: node.id.clone(),
            hostname: node.hostname.clone(),
            status: NodeStatus::Online,
            message: Some("Controller execution node".to_string()),
        }));
    }

    let (status, message) =
        verify_remote_node_access(&state, &node.ip, node.ssh_port, &node.ssh_user).await;

    let last_seen = if status == NodeStatus::Online {
        Some(chrono::Utc::now())
    } else {
        None
    };

    node.status = status.clone();
    node.status_message = message.clone();
    if last_seen.is_some() {
        node.last_seen = last_seen;
    }
    state.node_store.insert(&node)?;
    state.refresh_command_proxy();

    Ok(Json(NodeStatusResponse {
        id: node.id.clone(),
        hostname: node.hostname.clone(),
        status,
        message,
    }))
}

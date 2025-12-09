//! Cluster and node management API handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::core::{run_shell_command, SshCredential};
use crate::error::{AppError, AppResult};
use crate::models::{AddNodeRequest, BlockDevice, LsblkOutput, Node, NodeStatus};
use crate::state::AppState;

/// Health check response
#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
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
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
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
    let nodes = state.db.get_all_nodes()?;
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
    let existing_nodes = state.db.get_all_nodes()?;
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
    let ssh_user = req
        .ssh_user
        .clone()
        .unwrap_or(state.config.ssh.default_user.clone());

    // Dummy credential
    let credential = SshCredential::Password("ignored".to_string());

    let status = match state
        .ssh_manager
        .execute(&req.ip, ssh_port, &ssh_user, &credential, "echo ok")
        .await
    {
        Ok(output) if output.success() => NodeStatus::Online,
        _ => NodeStatus::Unknown,
    };

    // Create node
    let node = Node {
        id: uuid::Uuid::new_v4().to_string(),
        hostname: req.hostname,
        ip: req.ip,
        ssh_port: req.ssh_port.unwrap_or(state.config.ssh.default_port),
        ssh_user: req
            .ssh_user
            .unwrap_or(state.config.ssh.default_user.clone()),
        is_local: false,
        status: status.clone(),
        last_seen: if status == NodeStatus::Online {
            Some(chrono::Utc::now())
        } else {
            None
        },
    };

    // Store node to database
    state.db.insert_node(&node)?;

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
    state
        .db
        .get_node(&id)?
        .map(Json)
        .ok_or_else(|| AppError::NotFound(format!("Node {} not found", id)))
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
    if state.db.delete_node(&id)? {
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
    let node = state
        .db
        .get_node(&id)?
        .ok_or_else(|| AppError::NotFound(format!("Node {} not found", id)))?;

    // Get block devices using lsblk
    let lsblk_cmd = "lsblk -J -b -o NAME,SIZE,TYPE,MOUNTPOINT,FSTYPE,RO,MODEL,PATH";

    let output: LsblkOutput = if node.is_local {
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
    let all_disks = list_node_disks(State(state), Path(id)).await?;

    // Filter to only available disks
    let available: Vec<BlockDevice> = all_disks
        .0
        .into_iter()
        .filter(|d| d.is_available())
        .collect();

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
    let node = state
        .db
        .get_node(&id)?
        .ok_or_else(|| AppError::NotFound(format!("Node {} not found", id)))?;

    if node.is_local {
        state
            .db
            .update_node_status(&id, NodeStatus::Online, Some(chrono::Utc::now()))?;
        return Ok(Json(NodeStatusResponse {
            id: node.id.clone(),
            hostname: node.hostname.clone(),
            status: NodeStatus::Online,
            message: Some("Local node".to_string()),
        }));
    }

    // Use dummy credential
    let credential = SshCredential::Password("ignored".to_string());

    let (status, message) = match state
        .ssh_manager
        .execute(
            &node.ip,
            node.ssh_port,
            &node.ssh_user,
            &credential,
            "echo ok",
        )
        .await
    {
        Ok(output) if output.success() => (NodeStatus::Online, None),
        Ok(output) => (
            NodeStatus::Error,
            Some(format!("Command failed: {}", output.stderr)),
        ),
        Err(e) => (NodeStatus::Offline, Some(e.to_string())),
    };

    let last_seen = if status == NodeStatus::Online {
        Some(chrono::Utc::now())
    } else {
        None
    };
    state
        .db
        .update_node_status(&id, status.clone(), last_seen)?;

    Ok(Json(NodeStatusResponse {
        id: node.id.clone(),
        hostname: node.hostname.clone(),
        status,
        message,
    }))
}

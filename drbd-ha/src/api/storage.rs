use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use tracing::{info, warn};
use utoipa::ToSchema;

use crate::core::{
    validator, LvmProvider, SafetyChecker, StorageProvider, ZfsProvider, ZfsUtilsClient,
};
use crate::error::{AppError, AppResult};
use crate::models::storage::{
    CreateStoragePoolRequest, CreateStoragePoolResponse, CreateVolumeRequest, CreateVolumeResponse,
    ListStoragePoolResponse, StoragePool,
};
use crate::models::Node;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PoolKind {
    Lvm,
    Zfs,
}

impl PoolKind {
    fn parse(value: &str) -> AppResult<Self> {
        match value.trim().to_lowercase().as_str() {
            "lvm" => Ok(Self::Lvm),
            "zfs" => Ok(Self::Zfs),
            other => Err(AppError::Validation(format!(
                "Unsupported pool type '{}'. Expected 'lvm' or 'zfs'.",
                other
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Lvm => "lvm",
            Self::Zfs => "zfs",
        }
    }
}

fn ssh_credential() -> crate::core::SshCredential {
    crate::core::SshCredential::Password("ignored".to_string())
}

fn zfs_client_for_node(state: &Arc<AppState>, node: &Node) -> ZfsUtilsClient {
    if node.is_local {
        ZfsUtilsClient::new_local()
    } else {
        ZfsUtilsClient::new_remote(
            state.ssh_manager.clone(),
            node.ip.clone(),
            node.ssh_port,
            node.ssh_user.clone(),
            ssh_credential(),
        )
    }
}

fn lvm_provider_for_node(state: &Arc<AppState>, node: &Node, pool_name: String) -> LvmProvider {
    if node.is_local {
        LvmProvider::new_local(pool_name)
    } else {
        LvmProvider::new_remote(
            pool_name,
            state.ssh_manager.clone(),
            node.ip.clone(),
            node.ssh_port,
            node.ssh_user.clone(),
            ssh_credential(),
        )
    }
}

fn zfs_provider_for_node(state: &Arc<AppState>, node: &Node, pool_name: String) -> ZfsProvider {
    if node.is_local {
        ZfsProvider::new_local(pool_name)
    } else {
        ZfsProvider::new_remote(
            pool_name,
            state.ssh_manager.clone(),
            node.ip.clone(),
            node.ssh_port,
            node.ssh_user.clone(),
            ssh_credential(),
        )
    }
}

fn lvm_client_for_node(state: &Arc<AppState>, node: &Node) -> lvm_utils::LvmClient {
    if node.is_local {
        lvm_utils::LvmClient::new_local()
    } else {
        lvm_utils::LvmClient::new_remote(
            state.ssh_manager.to_inner().into(),
            node.ip.clone(),
            node.ssh_port,
            node.ssh_user.clone(),
            ssh_credential(),
        )
    }
}

fn storage_query_failed_is_non_fatal(error: &str) -> bool {
    let error = error.to_lowercase();
    error.contains("command not found")
        || error.contains("not found")
        || error.contains("no such file")
        || error.contains("could not find")
}

async fn list_lvm_pools_for_node(
    state: &Arc<AppState>,
    node: &Node,
) -> AppResult<Vec<StoragePool>> {
    let vg_infos = if node.is_local {
        match lvm_utils::list_vg_info().await {
            Ok(vgs) => vgs,
            Err(error) if storage_query_failed_is_non_fatal(&error.to_string()) => {
                warn!(
                    "Skipping LVM pool discovery on {}: {}",
                    node.hostname, error
                );
                return Ok(Vec::new());
            }
            Err(error) => return Err(AppError::Internal(error.to_string())),
        }
    } else {
        let client = lvm_client_for_node(state, node);
        match client.list_vg_info().await {
            Ok(vgs) => vgs,
            Err(error) if storage_query_failed_is_non_fatal(&error.to_string()) => {
                warn!(
                    "Skipping LVM pool discovery on {}: {}",
                    node.hostname, error
                );
                return Ok(Vec::new());
            }
            Err(error) => return Err(AppError::Internal(error.to_string())),
        }
    };

    Ok(vg_infos
        .into_iter()
        .map(|vg_info| StoragePool {
            id: vg_info.name.clone(),
            name: vg_info.name,
            node_id: node.id.clone(),
            device: "/dev/disk/by-path/placeholder".to_string(),
            total_size: vg_info.size,
            free_size: vg_info.free,
        })
        .collect())
}

async fn list_zfs_pools_for_node(
    state: &Arc<AppState>,
    node: &Node,
) -> AppResult<Vec<StoragePool>> {
    let client = zfs_client_for_node(state, node);
    let pools = match client.list_pool_info().await {
        Ok(pools) => pools,
        Err(error) if storage_query_failed_is_non_fatal(&error.to_string()) => {
            warn!(
                "Skipping ZFS pool discovery on {}: {}",
                node.hostname, error
            );
            return Ok(Vec::new());
        }
        Err(error) => return Err(AppError::Internal(error.to_string())),
    };

    let mut storage_pools = Vec::with_capacity(pools.len());
    for pool in pools {
        let devices = match client.get_pool_devices(&pool.name).await {
            Ok(devices) => devices,
            Err(error) => {
                warn!(
                    "Failed to query zpool devices for {} on {}: {}",
                    pool.name, node.hostname, error
                );
                Vec::new()
            }
        };

        storage_pools.push(StoragePool {
            id: pool.name.clone(),
            name: pool.name,
            node_id: node.id.clone(),
            device: if devices.is_empty() {
                "/dev/disk/by-id/unknown".to_string()
            } else {
                devices.join(",")
            },
            total_size: pool.size,
            free_size: pool.free,
        });
    }

    Ok(storage_pools)
}

fn resolve_pool_creation_targets(
    req: &CreateStoragePoolRequest,
    all_nodes: &[Node],
) -> AppResult<Vec<(Node, String)>> {
    if !req.node_devices.is_empty() {
        let mut targets = Vec::with_capacity(req.node_devices.len());
        for (node_id, device) in &req.node_devices {
            let node = all_nodes
                .iter()
                .find(|n| &n.id == node_id)
                .ok_or_else(|| AppError::NotFound(format!("Node '{}' not found", node_id)))?;
            targets.push((node.clone(), device.clone()));
        }
        return Ok(targets);
    }

    let device = req.device.clone().ok_or_else(|| {
        AppError::Validation(
            "At least one node must be specified via 'node_devices', or provide 'device' for a single target node."
                .to_string(),
        )
    })?;

    if let Some(local_node) = all_nodes.iter().find(|node| node.is_local) {
        return Ok(vec![(local_node.clone(), device)]);
    }

    if all_nodes.len() == 1 {
        return Ok(vec![(all_nodes[0].clone(), device)]);
    }

    Err(AppError::Validation(
        "The legacy 'device' field is only supported when there is a single target node. Use 'node_devices' for multi-node requests."
            .to_string(),
    ))
}

fn execution_node_for_controller(state: &Arc<AppState>, all_nodes: &[Node]) -> Option<Node> {
    all_nodes
        .iter()
        .find(|node| node.is_local)
        .or_else(|| all_nodes.iter().find(|node| state.is_controller_node(node)))
        .cloned()
}

async fn detect_pool_kind_on_node(
    state: &Arc<AppState>,
    node: &Node,
    pool_id: &str,
    requested_kind: Option<PoolKind>,
) -> AppResult<PoolKind> {
    if let Some(kind) = requested_kind {
        return match kind {
            PoolKind::Lvm => {
                let client = lvm_client_for_node(state, node);
                let exists = client
                    .get_vg_info(pool_id)
                    .await
                    .map_err(|error| AppError::Internal(error.to_string()))?
                    .is_some();
                if exists {
                    Ok(PoolKind::Lvm)
                } else {
                    Err(AppError::NotFound(format!(
                        "LVM pool '{}' not found on node '{}'",
                        pool_id, node.hostname
                    )))
                }
            }
            PoolKind::Zfs => {
                let client = zfs_client_for_node(state, node);
                let exists = client
                    .get_pool_info(pool_id)
                    .await
                    .map_err(|error| AppError::Internal(error.to_string()))?
                    .is_some();
                if exists {
                    Ok(PoolKind::Zfs)
                } else {
                    Err(AppError::NotFound(format!(
                        "ZFS pool '{}' not found on node '{}'",
                        pool_id, node.hostname
                    )))
                }
            }
        };
    }

    let lvm_exists = lvm_client_for_node(state, node)
        .get_vg_info(pool_id)
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?
        .is_some();
    let zfs_exists = zfs_client_for_node(state, node)
        .get_pool_info(pool_id)
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?
        .is_some();

    match (lvm_exists, zfs_exists) {
        (true, false) => Ok(PoolKind::Lvm),
        (false, true) => Ok(PoolKind::Zfs),
        (true, true) => Err(AppError::Validation(format!(
            "Pool '{}' exists as both LVM and ZFS on node '{}'. Set pool_type explicitly.",
            pool_id, node.hostname
        ))),
        (false, false) => Err(AppError::NotFound(format!(
            "Storage pool '{}' not found on node '{}'",
            pool_id, node.hostname
        ))),
    }
}

/// GET /api/v1/pools
#[utoipa::path(
    get,
    path = "/api/v1/pools",
    tag = "storage",
    responses(
        (status = 200, description = "List storage pools", body = ListStoragePoolResponse)
    )
)]
pub async fn list_pools(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<ListStoragePoolResponse>> {
    info!("Listing storage pools from managed nodes");

    let mut nodes = state.node_store.get_all()?;
    if nodes.is_empty() {
        nodes.push(Node {
            id: "local".to_string(),
            hostname: state.controller_hostname(),
            ip: "127.0.0.1".to_string(),
            ssh_port: state.config.ssh.default_port,
            ssh_user: state.config.ssh.default_user.clone(),
            is_local: true,
            status: crate::models::NodeStatus::Unknown,
            status_message: None,
            last_seen: None,
        });
    }

    let mut pools = Vec::new();
    for node in &nodes {
        pools.extend(list_lvm_pools_for_node(&state, node).await?);
        pools.extend(list_zfs_pools_for_node(&state, node).await?);
    }
    pools.sort_by(|left, right| {
        left.node_id
            .cmp(&right.node_id)
            .then_with(|| left.name.cmp(&right.name))
    });

    Ok(Json(ListStoragePoolResponse { pools }))
}

/// POST /api/v1/pools
#[utoipa::path(
    post,
    path = "/api/v1/pools",
    tag = "storage",
    request_body = CreateStoragePoolRequest,
    responses(
        (status = 201, description = "Storage pool created", body = CreateStoragePoolResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Pool already exists")
    )
)]
pub async fn create_pool(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateStoragePoolRequest>,
) -> AppResult<(StatusCode, Json<Vec<CreateStoragePoolResponse>>)> {
    info!("Creating storage pool: {:?}", req);

    let pool_kind = PoolKind::parse(&req.pool_type)?;

    validator::validate_resource_name(&req.name)?;

    let all_nodes = state.node_store.get_all()?;
    let targets = resolve_pool_creation_targets(&req, &all_nodes)?;
    let mut responses = Vec::new();
    let safety_checker = SafetyChecker::new(state.ssh_manager.clone());

    for (node, device) in targets {
        validator::validate_block_device(&device)?;
        let check_result = if node.is_local {
            safety_checker.check_device_for_drbd(&device).await?
        } else {
            safety_checker
                .check_remote_device_for_drbd(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &ssh_credential(),
                    &device,
                )
                .await?
        };
        if let Some(err) = check_result.to_error() {
            return Err(err);
        }

        let (total_size, free_size) = match pool_kind {
            PoolKind::Lvm => {
                let client = lvm_client_for_node(&state, &node);
                if let Some(existing_vg) = client
                    .get_vg_info(&req.name)
                    .await
                    .map_err(|error| AppError::Internal(error.to_string()))?
                {
                    return Err(AppError::AlreadyExists(format!(
                        "Storage pool '{}' (LVM VG) already exists on node '{}' with size {} bytes",
                        req.name, node.hostname, existing_vg.size
                    )));
                }

                let lvm_provider = lvm_provider_for_node(&state, &node, req.name.clone());
                lvm_provider.init_pool(&device).await.map_err(|error| {
                    AppError::Internal(format!(
                        "Node {}: Failed to create LVM pool: {}",
                        node.hostname, error
                    ))
                })?;

                match client.get_vg_info(&req.name).await {
                    Ok(Some(info)) => (info.size, info.free),
                    Ok(None) => (0, 0),
                    Err(error) => {
                        warn!("Failed to get VG info on {}: {}", node.hostname, error);
                        (0, 0)
                    }
                }
            }
            PoolKind::Zfs => {
                let client = zfs_client_for_node(&state, &node);
                if let Some(existing_pool) = client
                    .get_pool_info(&req.name)
                    .await
                    .map_err(|error| AppError::Internal(error.to_string()))?
                {
                    return Err(AppError::AlreadyExists(format!(
                        "Storage pool '{}' (zpool) already exists on node '{}' with size {} bytes",
                        req.name, node.hostname, existing_pool.size
                    )));
                }

                let zfs_provider = zfs_provider_for_node(&state, &node, req.name.clone());
                zfs_provider.init_pool(&device).await.map_err(|error| {
                    AppError::Internal(format!(
                        "Node {}: Failed to create zpool: {}",
                        node.hostname, error
                    ))
                })?;

                match client.get_pool_info(&req.name).await {
                    Ok(Some(info)) => (info.size, info.free),
                    Ok(None) => (0, 0),
                    Err(error) => {
                        warn!("Failed to get zpool info on {}: {}", node.hostname, error);
                        (0, 0)
                    }
                }
            }
        };

        responses.push(CreateStoragePoolResponse {
            id: req.name.clone(),
            name: req.name.clone(),
            node_id: node.id.clone(),
            device: device.clone(),
            total_size,
            free_size,
        });
    }

    info!(
        "Storage pool '{}' ({}) created on {} nodes",
        req.name,
        pool_kind.as_str(),
        responses.len()
    );

    Ok((StatusCode::CREATED, Json(responses)))
}

/// POST /api/v1/pools/:pool_id/volumes
#[utoipa::path(
    post,
    path = "/api/v1/pools/{pool_id}/volumes",
    tag = "storage",
    params(
        ("pool_id" = String, Path, description = "Storage Pool ID")
    ),
    request_body = CreateVolumeRequest,
    responses(
        (status = 201, description = "Volume created", body = CreateVolumeResponse),
        (status = 400, description = "Validation error"),
        (status = 404, description = "Pool not found")
    )
)]
pub async fn create_volume(
    State(state): State<Arc<AppState>>,
    Path(pool_id): Path<String>,
    Json(req): Json<CreateVolumeRequest>,
) -> AppResult<(StatusCode, Json<CreateVolumeResponse>)> {
    info!(
        "Creating volume '{}' in pool '{}' with size {}GB",
        req.name, pool_id, req.size_gb
    );

    // Validate inputs
    validator::validate_resource_name(&req.name)?; // Using resource name validator for LV name
    if req.size_gb == 0 {
        return Err(AppError::Validation(
            "Volume size must be greater than 0".to_string(),
        ));
    }

    let all_nodes = state.node_store.get_all()?;
    let node = execution_node_for_controller(&state, &all_nodes).unwrap_or(Node {
        id: "local".to_string(),
        hostname: state.controller_hostname(),
        ip: "127.0.0.1".to_string(),
        ssh_port: state.config.ssh.default_port,
        ssh_user: state.config.ssh.default_user.clone(),
        is_local: true,
        status: crate::models::NodeStatus::Unknown,
        status_message: None,
        last_seen: None,
    });
    let requested_kind = req.pool_type.as_deref().map(PoolKind::parse).transpose()?;
    let pool_kind = detect_pool_kind_on_node(&state, &node, &pool_id, requested_kind).await?;
    let requested_size_bytes = req.size_gb * 1024 * 1024 * 1024;

    let device_path = match pool_kind {
        PoolKind::Lvm => {
            let client = lvm_client_for_node(&state, &node);
            let vg_info = client
                .get_vg_info(&pool_id)
                .await
                .map_err(|error| AppError::Internal(error.to_string()))?
                .ok_or_else(|| {
                    AppError::NotFound(format!("Storage pool '{}' not found", pool_id))
                })?;

            let existing_lvs = client.list_lvs().await.unwrap_or_default();
            if existing_lvs
                .iter()
                .any(|lv| lv.name == req.name && lv.vg_name == pool_id)
            {
                return Err(AppError::AlreadyExists(format!(
                    "Volume with name '{}' already exists in pool '{}'.",
                    req.name, pool_id
                )));
            }

            if requested_size_bytes > vg_info.free {
                return Err(AppError::Validation(format!(
                    "Requested size ({}GB) exceeds available free space ({} bytes) in pool '{}'",
                    req.size_gb, vg_info.free, pool_id
                )));
            }

            let provider = lvm_provider_for_node(&state, &node, pool_id.clone());
            provider
                .create_volume(&req.name, req.size_gb)
                .await
                .map_err(|error| AppError::Internal(error.to_string()))?
        }
        PoolKind::Zfs => {
            let client = zfs_client_for_node(&state, &node);
            let pool_info = client
                .get_pool_info(&pool_id)
                .await
                .map_err(|error| AppError::Internal(error.to_string()))?
                .ok_or_else(|| {
                    AppError::NotFound(format!("Storage pool '{}' not found", pool_id))
                })?;
            let dataset_name = format!("{}/{}", pool_id, req.name);
            let datasets = client
                .list_datasets(Some(&pool_id))
                .await
                .map_err(|error| AppError::Internal(error.to_string()))?;
            if datasets.iter().any(|dataset| dataset.name == dataset_name) {
                return Err(AppError::AlreadyExists(format!(
                    "Volume with name '{}' already exists in pool '{}'.",
                    req.name, pool_id
                )));
            }

            if requested_size_bytes > pool_info.free {
                return Err(AppError::Validation(format!(
                    "Requested size ({}GB) exceeds available free space ({} bytes) in pool '{}'",
                    req.size_gb, pool_info.free, pool_id
                )));
            }

            let provider = zfs_provider_for_node(&state, &node, pool_id.clone());
            provider
                .create_volume(&req.name, req.size_gb)
                .await
                .map_err(|error| AppError::Internal(error.to_string()))?
        }
    };

    info!(
        "Volume '{}' ({}GB) created successfully in pool '{}' ({}) on node '{}'. Device path: {}",
        req.name,
        req.size_gb,
        pool_id,
        pool_kind.as_str(),
        node.hostname,
        device_path
    );

    Ok((
        StatusCode::CREATED,
        Json(CreateVolumeResponse {
            id: uuid::Uuid::new_v4().to_string(),
            name: req.name,
            pool_id: pool_id.to_string(),
            size_gb: req.size_gb,
            device_path,
        }),
    ))
}

/// Zpool status check response
#[derive(Serialize, ToSchema)]
pub struct ZpoolCheckResponse {
    pub installed: bool,
    pub available: bool,
    pub version: Option<String>,
    pub message: String,
    pub pools: Vec<ZpoolInfo>,
}

/// Information about a zpool
#[derive(Serialize, ToSchema)]
pub struct ZpoolInfo {
    pub name: String,
    pub size: String,
    pub capacity: String,
    pub health: String,
}

/// GET /api/v1/storage/zpool/check
#[utoipa::path(
    get,
    path = "/api/v1/storage/zpool/check",
    tag = "storage",
    responses(
        (status = 200, description = "Zpool status checked", body = ZpoolCheckResponse)
    )
)]
pub async fn check_zpool(
    State(_state): State<Arc<AppState>>,
) -> AppResult<Json<ZpoolCheckResponse>> {
    info!("Checking ZFS/zpool availability on local node");

    // Use zfs-utils to check local zpool
    let result = crate::core::check_zpool_local()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check zpool: {}", e)))?;

    // Convert zfs-utils result to API response
    let pools = result
        .pools
        .into_iter()
        .map(|p| ZpoolInfo {
            name: p.name,
            size: p.size,
            capacity: p.capacity,
            health: p.health,
        })
        .collect();

    Ok(Json(ZpoolCheckResponse {
        installed: result.installed,
        available: result.available,
        version: result.version,
        message: result.message,
        pools,
    }))
}

/// GET /api/v1/storage/zpool/check/{node_id}
#[utoipa::path(
    get,
    path = "/api/v1/storage/zpool/check/{node_id}",
    tag = "storage",
    params(
        ("node_id" = String, Path, description = "Node ID to check")
    ),
    responses(
        (status = 200, description = "Zpool status checked on remote node", body = ZpoolCheckResponse),
        (status = 404, description = "Node not found")
    )
)]
pub async fn check_zpool_on_node(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
) -> AppResult<Json<ZpoolCheckResponse>> {
    let node = state
        .node_store
        .get(&node_id)?
        .ok_or_else(|| AppError::NotFound(format!("Node {} not found", node_id)))?;

    info!("Checking ZFS/zpool availability on node: {}", node.hostname);

    let zfs_client = zfs_client_for_node(&state, &node);

    let result = zfs_client.check_zpool().await.map_err(|e| {
        AppError::Internal(format!("Failed to check zpool on {}: {}", node.hostname, e))
    })?;

    // Convert zfs-utils result to API response
    let pools = result
        .pools
        .into_iter()
        .map(|p| ZpoolInfo {
            name: p.name,
            size: p.size,
            capacity: p.capacity,
            health: p.health,
        })
        .collect();

    Ok(Json(ZpoolCheckResponse {
        installed: result.installed,
        available: result.available,
        version: result.version,
        message: result.message,
        pools,
    }))
}

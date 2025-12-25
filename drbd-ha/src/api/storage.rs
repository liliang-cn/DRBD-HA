use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use tracing::info;
use utoipa::ToSchema;

use crate::core::{get_vg_info, list_vg_info, run_shell_command, validator, LvmProvider, SafetyChecker, StorageProvider};
use crate::error::{AppError, AppResult};
use crate::models::storage::{
    CreateStoragePoolRequest, CreateStoragePoolResponse, CreateVolumeRequest, CreateVolumeResponse,
    ListStoragePoolResponse, StoragePool,
};
use crate::state::AppState;

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
    State(_state): State<Arc<AppState>>,
) -> AppResult<Json<ListStoragePoolResponse>> {
    info!("Listing storage pools (LVM Volume Groups) - scanning from LVM");

    // Scan LVM directly for all volume groups
    let vg_infos = list_vg_info().await?;

    let pools: Vec<StoragePool> = vg_infos
        .into_iter()
        .map(|vg_info| StoragePool {
            id: vg_info.name.clone(), // Use VG name as ID
            name: vg_info.name,
            node_id: "local".to_string(), // Local for now
            device: "/dev/disk/by-path/placeholder".to_string(), // Placeholder
            total_size: vg_info.size,
            free_size: vg_info.free,
        })
        .collect();

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

    if req.pool_type.to_lowercase() != "lvm" {
        return Err(AppError::Validation(
            "Only 'lvm' pool type is supported for now.".to_string(),
        ));
    }

    validator::validate_resource_name(&req.name)?;

    if req.node_devices.is_empty() {
        return Err(AppError::Validation(
            "At least one node must be specified via 'node_devices'".to_string(),
        ));
    }

    let all_nodes = state.node_store.get_all()?;
    let mut responses = Vec::new();

    for (node_id, device) in &req.node_devices {
        validator::validate_block_device(device)?;

        let node = all_nodes
            .iter()
            .find(|n| &n.id == node_id)
            .ok_or_else(|| AppError::NotFound(format!("Node '{}' not found", node_id)))?;

        // Check if VG already exists in LVM
        if let Some(existing_vg) = get_vg_info(&req.name).await? {
            return Err(AppError::AlreadyExists(format!(
                "Storage pool '{}' (VG) already exists with size {}GB",
                req.name, existing_vg.size
            )));
        }

        let safety_checker = SafetyChecker::new(state.ssh_manager.clone());
        let check_result = safety_checker.check_device_for_drbd(device).await?;
        if let Some(err) = check_result.to_error() {
            return Err(err);
        }

        let lvm_provider = if node.is_local {
            LvmProvider::new_local(req.name.clone())
        } else {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            LvmProvider::new_remote(
                req.name.clone(),
                state.ssh_manager.clone(),
                node.ip.clone(),
                node.ssh_port,
                node.ssh_user.clone(),
                credential,
            )
        };

        // Init pool
        if let Err(e) = lvm_provider.init_pool(device).await {
            return Err(AppError::Internal(format!(
                "Node {}: Failed to create VG: {}",
                node.hostname, e
            )));
        }

        // Get sizes (this runs vgdisplay)
        use crate::core::lvm_utils::LvmClient;
        let client = if node.is_local {
            LvmClient::new_local()
        } else {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            LvmClient::new_remote(
                state.ssh_manager.clone(),
                node.ip.clone(),
                node.ssh_port,
                node.ssh_user.clone(),
                credential,
            )
        };

        let (total_size, free_size) = match client.get_vg_info(&req.name).await {
            Ok(Some(info)) => (info.size, info.free),
            Ok(None) => (0, 0), // Created but not found?
            Err(e) => {
                tracing::warn!("Failed to get VG info on {}: {}", node.hostname, e);
                (0, 0)
            }
        };

        // Pool created in LVM, just return the info
        responses.push(CreateStoragePoolResponse {
            id: req.name.clone(), // Use VG name as ID
            name: req.name.clone(),
            node_id: node.id.clone(),
            device: device.clone(),
            total_size,
            free_size,
        });
    }

    info!(
        "Storage pool '{}' created on {} nodes",
        req.name,
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
    State(_state): State<Arc<AppState>>,
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

    // Get pool info directly from LVM (pool_id is VG name)
    let vg_info = get_vg_info(&pool_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Storage pool '{}' not found", pool_id)))?;

    // Check if LV already exists in LVM
    let existing_lvs = crate::core::list_lvs().await.unwrap_or_default();
    if existing_lvs.iter().any(|lv| lv.name == req.name && lv.vg_name == pool_id) {
        return Err(AppError::AlreadyExists(format!(
            "Volume with name '{}' already exists in pool '{}'.",
            req.name, pool_id
        )));
    }

    // Check free space
    if (req.size_gb * 1024 * 1024 * 1024) > vg_info.free {
        return Err(AppError::Validation(format!(
            "Requested size ({}GB) exceeds available free space ({} bytes) in pool '{}'",
            req.size_gb, vg_info.free, pool_id
        )));
    }

    let lvm_provider = LvmProvider::new_local(pool_id.clone());
    let device_path = lvm_provider.create_volume(&req.name, req.size_gb).await?;

    info!(
        "Volume '{}' ({}GB) created successfully in pool '{}'. Device path: {}",
        req.name, req.size_gb, pool_id, device_path
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
pub async fn check_zpool(State(_state): State<Arc<AppState>>) -> AppResult<Json<ZpoolCheckResponse>> {
    info!("Checking ZFS/zpool availability");

    // First check if zpool command exists
    let which_cmd = "which zpool";
    let check_installed = run_shell_command(which_cmd, "Check if zpool is installed").await;

    let installed = check_installed.is_ok() && check_installed.as_ref().map(|o| o.exit_code == 0).unwrap_or(false);

    if !installed {
        return Ok(Json(ZpoolCheckResponse {
            installed: false,
            available: false,
            version: None,
            message: "zpool command not found. ZFS is not installed on this system.".to_string(),
            pools: vec![],
        }));
    }

    // Check zpool version
    let version_cmd = "zpool version 2>&1 | head -n 1";
    let version_output = run_shell_command(version_cmd, "Get zpool version").await;
    let version = if version_output.is_ok() {
        version_output.ok().and_then(|o| {
            let stdout = o.stdout.trim();
            if stdout.is_empty() {
                None
            } else {
                Some(stdout.to_string())
            }
        })
    } else {
        None
    };

    // Try to list pools to check if zpool is functional
    let list_cmd = "zpool list -H -o name,size,capacity,health 2>/dev/null";
    let list_output = run_shell_command(list_cmd, "List zpools").await;

    let available = list_output.is_ok();
    let mut pools = vec![];

    if let Ok(output) = list_output {
        // Parse zpool list output
        // Format: name\tsize\tcapacity\thealth
        for line in output.stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 4 {
                pools.push(ZpoolInfo {
                    name: parts[0].to_string(),
                    size: parts[1].to_string(),
                    capacity: parts[2].to_string(),
                    health: parts[3].to_string(),
                });
            }
        }
    }

    let message = if available {
        format!(
            "ZFS is installed and available. Found {} pool(s).",
            pools.len()
        )
    } else {
        "ZFS is installed but zpool command failed. ZFS kernel module may not be loaded.".to_string()
    };

    Ok(Json(ZpoolCheckResponse {
        installed: true,
        available,
        version,
        message,
        pools,
    }))
}

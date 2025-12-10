use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use tracing::info;

use crate::core::{get_vg_info, validator, LvmProvider, SafetyChecker, StorageProvider};
use crate::error::{AppError, AppResult};
use crate::models::storage::{
    CreateStoragePoolRequest, CreateStoragePoolResponse, CreateVolumeRequest, CreateVolumeResponse,
    ListStoragePoolResponse, StoragePool, Volume,
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
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<ListStoragePoolResponse>> {
    info!("Listing storage pools (LVM Volume Groups)");
    let mut pools = state.db.get_all_storage_pools()?;

    // Update pool info from live LVM data
    for pool in &mut pools {
        // Assume local for now
        if let Some(vg_info) = get_vg_info(&pool.name).await? {
            pool.total_size = vg_info.size;
            pool.free_size = vg_info.free;
            // Update in DB
            state
                .db
                .update_storage_pool_sizes(&pool.id, pool.total_size, pool.free_size)?;
        } else {
            // VG not found, mark as error or remove from list?
            // For now, set sizes to 0
            pool.total_size = 0;
            pool.free_size = 0;
            tracing::warn!(
                "LVM Volume Group '{}' for pool '{}' not found on local system.",
                pool.name,
                pool.id
            );
        }
    }

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

    let all_nodes = state.db.get_all_nodes()?;
    let mut responses = Vec::new();

    for (node_id, device) in &req.node_devices {
        validator::validate_block_device(device)?;

        let node = all_nodes
            .iter()
            .find(|n| &n.id == node_id)
            .ok_or_else(|| AppError::NotFound(format!("Node '{}' not found", node_id)))?;

        let existing_pools = state.db.get_all_storage_pools()?;
        if existing_pools
            .iter()
            .any(|p| p.name == req.name && p.node_id == node.id)
        {
            return Err(AppError::AlreadyExists(format!(
                "Storage pool '{}' already exists on node '{}'",
                req.name, node.hostname
            )));
        }

        let safety_checker = SafetyChecker::new(state.ssh_manager.clone());
        let check_result = safety_checker.check_device_for_drbd(device).await?;
        if let Some(err) = check_result.to_error() {
            return Err(err);
        }

        let lvm_provider: LvmProvider;
        if node.is_local {
            lvm_provider = LvmProvider::new_local(req.name.clone());
        } else {
            let credential = crate::core::SshCredential::Password("ignored".to_string());
            lvm_provider = LvmProvider::new_remote(
                req.name.clone(),
                state.ssh_manager.clone(),
                node.ip.clone(),
                node.ssh_port,
                node.ssh_user.clone(),
                credential,
            );
        }

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

        // Insert into DB
        let new_pool = StoragePool {
            id: uuid::Uuid::new_v4().to_string(),
            name: req.name.clone(),
            node_id: node.id.clone(),
            device: device.clone(),
            total_size,
            free_size,
        };

        if let Err(e) = state.db.insert_storage_pool(&new_pool, &node.id) {
            return Err(AppError::Internal(format!(
                "Node {}: Created VG but DB insert failed: {}",
                node.hostname, e
            )));
        } else {
            responses.push(CreateStoragePoolResponse {
                id: new_pool.id,
                name: new_pool.name,
                node_id: new_pool.node_id,
                device: new_pool.device,
                total_size: new_pool.total_size,
                free_size: new_pool.free_size,
            });
        }
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

    // Retrieve pool details from database
    let mut pool = state
        .db
        .get_storage_pool(&pool_id)?
        .ok_or_else(|| AppError::NotFound(format!("Storage pool '{}' not found", pool_id)))?;

    // Check if volume name already exists in this pool
    if state
        .db
        .get_all_volumes_in_pool(&pool_id)?
        .iter()
        .any(|v| v.name == req.name)
    {
        return Err(AppError::AlreadyExists(format!(
            "Volume with name '{}' already exists in pool '{}'.",
            req.name, pool.name
        )));
    }

    // Get current VG info to check free space
    let vg_info = get_vg_info(&pool.name)
        .await?
        .ok_or_else(|| AppError::Internal(format!("LVM VG '{}' not found on system", pool.name)))?;

    if (req.size_gb as u64 * 1024 * 1024 * 1024) > vg_info.free {
        return Err(AppError::Validation(format!(
            "Requested size ({}GB) exceeds available free space ({} bytes) in pool '{}'",
            req.size_gb, vg_info.free, pool.name
        )));
    }

    let lvm_provider = LvmProvider::new_local(pool.name.clone());
    let device_path = lvm_provider.create_volume(&req.name, req.size_gb).await?;

    let new_volume = Volume {
        id: uuid::Uuid::new_v4().to_string(), // Generate ID for volume
        pool_id: pool.id.clone(),
        name: req.name,
        size_gb: req.size_gb,
        device_path: device_path.clone(),
        drbd_res: None, // Will be updated when associated with a DRBD resource
    };

    // Save new_volume to database
    state.db.insert_volume(&new_volume)?;

    // Update pool's free size
    let updated_vg_info = get_vg_info(&pool.name).await?.ok_or_else(|| {
        AppError::Internal(format!("Failed to re-read VG info for '{}'", pool.name))
    })?;
    pool.free_size = updated_vg_info.free;
    state
        .db
        .update_storage_pool_sizes(&pool.id, pool.total_size, pool.free_size)?;

    info!(
        "Volume '{}' ({}GB) created successfully in pool '{}'. Device path: {}",
        new_volume.name, new_volume.size_gb, pool.name, new_volume.device_path
    );

    Ok((
        StatusCode::CREATED,
        Json(CreateVolumeResponse {
            id: new_volume.id,
            name: new_volume.name,
            pool_id: new_volume.pool_id, // Return pool_id
            size_gb: new_volume.size_gb,
            device_path: new_volume.device_path,
        }),
    ))
}

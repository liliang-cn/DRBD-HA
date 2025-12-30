//! API endpoints for editing drbd-reactor TOML configuration files

use axum::{
    extract::{Path as AxumPath, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path as StdPath;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::core::{cluster_sync::HaSyncConfig, systemd_ctrl::SystemdController};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Request body for updating a profile's TOML configuration
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateTomlRequest {
    /// The new TOML content to write
    pub content: String,
}

/// Response containing the TOML file content
#[derive(Debug, Serialize, ToSchema)]
pub struct TomlContentResponse {
    /// Profile name
    pub profile: String,
    /// The TOML file content
    pub content: String,
    /// Path to the TOML file
    pub path: String,
}

/// Response for sync operation
#[derive(Debug, Serialize, ToSchema)]
pub struct SyncTomlResponse {
    /// Profile name
    pub profile: String,
    /// Nodes that were successfully synced
    pub synced_nodes: Vec<String>,
    /// Overall message
    pub message: String,
    /// Whether the operation was successful
    pub success: bool,
}

/// GET /api/v1/ha/profiles/{id}/toml
///
/// Get the raw TOML configuration content for a specific HA profile
#[utoipa::path(
    get,
    path = "/api/v1/ha/profiles/{id}/toml",
    tag = "HA",
    params(
        ("id" = String, Path, description = "Profile ID or name")
    ),
    responses(
        (status = 200, description = "TOML file content", body = TomlContentResponse),
        (status = 404, description = "Profile or TOML file not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_profile_toml(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> AppResult<Json<TomlContentResponse>> {
    let config_path = state.reactor_config_path(&id);

    if !StdPath::new(&config_path).exists() {
        return Err(AppError::NotFound(format!(
            "TOML configuration file not found for profile '{}': {}",
            id, config_path
        )));
    }

    let content = fs::read_to_string(&config_path).map_err(|e| {
        AppError::Internal(format!("Failed to read TOML file '{}': {}", config_path, e))
    })?;

    Ok(Json(TomlContentResponse {
        profile: id,
        content,
        path: config_path,
    }))
}

/// PUT /api/v1/ha/profiles/{id}/toml
///
/// Update the TOML configuration content for a specific HA profile
#[utoipa::path(
    put,
    path = "/api/v1/ha/profiles/{id}/toml",
    tag = "HA",
    params(
        ("id" = String, Path, description = "Profile ID or name")
    ),
    request_body = UpdateTomlRequest,
    responses(
        (status = 200, description = "TOML file updated successfully", body = TomlContentResponse),
        (status = 400, description = "Invalid TOML content"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn update_profile_toml(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
    Json(request): Json<UpdateTomlRequest>,
) -> AppResult<Json<TomlContentResponse>> {
    // Validate the TOML content before writing
    request.content.parse::<toml::Value>().map_err(|e| {
        AppError::Validation(format!("Invalid TOML content: {}", e))
    })?;

    let config_path = state.reactor_config_path(&id);

    // Write the TOML content
    fs::write(&config_path, &request.content).map_err(|e| {
        AppError::Internal(format!("Failed to write TOML file '{}': {}", config_path, e))
    })?;

    tracing::info!("Updated TOML configuration for profile '{}': {}", id, config_path);

    Ok(Json(TomlContentResponse {
        profile: id,
        content: request.content,
        path: config_path,
    }))
}

/// POST /api/v1/ha/profiles/{id}/toml/sync
///
/// Sync the TOML configuration file to all nodes in the cluster
#[utoipa::path(
    post,
    path = "/api/v1/ha/profiles/{id}/toml/sync",
    tag = "HA",
    params(
        ("id" = String, Path, description = "Profile ID or name")
    ),
    responses(
        (status = 200, description = "TOML file synced successfully", body = SyncTomlResponse),
        (status = 404, description = "TOML file not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn sync_profile_toml(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> AppResult<Json<SyncTomlResponse>> {
    let config_path = state.reactor_config_path(&id);

    // Check if the TOML file exists
    if !StdPath::new(&config_path).exists() {
        return Err(AppError::NotFound(format!(
            "TOML configuration file not found for profile '{}': {}",
            id, config_path
        )));
    }

    // Read the TOML content
    let content = fs::read_to_string(&config_path).map_err(|e| {
        AppError::Internal(format!("Failed to read TOML file '{}': {}", config_path, e))
    })?;

    // Create cluster sync instance
    let cluster_sync = crate::core::cluster_sync::ClusterSync::new(
        state.ssh_manager.clone(),
        state.node_store.clone(),
        state.credentials.clone(),
    );

    // Prepare sync config (only sync the TOML file)
    let sync_config = HaSyncConfig {
        drbd_resource_config: None,
        mount_unit: None,
        service_overrides: vec![],
        promoter_config: (config_path, content),
    };

    // Sync to all nodes
    let synced_nodes = cluster_sync.sync_ha_config(&sync_config).await.map_err(|e| {
        AppError::Internal(format!("Failed to sync TOML configuration: {}", e))
    })?;

    // Reload systemd on local node
    let systemd = SystemdController::new().await;
    if let Ok(sys) = systemd {
        let _ = sys.daemon_reload().await;
        let _ = sys.reload("drbd-reactor.service").await;
    }

    let success = !synced_nodes.is_empty();
    let total_nodes = state.node_store.get_all()?.len();
    let message = if success {
        format!(
            "Successfully synced TOML configuration to {}/{} nodes",
            synced_nodes.len(),
            total_nodes
        )
    } else {
        "No remote nodes to sync".to_string()
    };

    tracing::info!(
        "Synced TOML configuration for profile '{}' to {} nodes: {:?}",
        id,
        synced_nodes.len(),
        synced_nodes
    );

    Ok(Json(SyncTomlResponse {
        profile: id,
        synced_nodes,
        message,
        success,
    }))
}


//! API endpoints for editing drbd-reactor TOML configuration files

use axum::{
    extract::{Path as AxumPath, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::core::{cluster_sync::HaSyncConfig, systemd_ctrl::SystemdController, SshCredential};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use systemd_utils::RemoteSystemdController;

use toml;

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

/// Response for update start array operation
#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateStartArrayResponse {
    /// Profile name
    pub profile: String,
    /// The updated TOML file content
    pub content: String,
    /// Path to the TOML file
    pub path: String,
    /// Nodes that were successfully synced
    pub synced_nodes: Vec<String>,
    /// Overall success status
    pub success: bool,
    /// Status message
    pub message: String,
}

/// Request body for updating only the start array
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateStartArrayRequest {
    /// New start array items
    pub start: Vec<String>,
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

    if !state
        .controller_file_exists(&config_path)
        .await
        .unwrap_or(false)
    {
        return Err(AppError::NotFound(format!(
            "TOML configuration file not found for profile '{}': {}",
            id, config_path
        )));
    }

    let content = state.read_controller_file(&config_path).await?;

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
    toml::from_str::<toml::Value>(&request.content)
        .map_err(|e| AppError::Validation(format!("Invalid TOML content: {}", e)))?;

    let config_path = state.reactor_config_path(&id);

    // Write the TOML content
    state.write_controller_file(&config_path, &request.content).await?;

    tracing::info!(
        "Updated TOML configuration for profile '{}': {}",
        id,
        config_path
    );

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
    if !state
        .controller_file_exists(&config_path)
        .await
        .unwrap_or(false)
    {
        return Err(AppError::NotFound(format!(
            "TOML configuration file not found for profile '{}': {}",
            id, config_path
        )));
    }

    // Read the TOML content
    let content = state.read_controller_file(&config_path).await?;

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
    let synced_nodes = cluster_sync
        .sync_ha_config(&sync_config)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to sync TOML configuration: {}", e)))?;

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

/// PUT /api/v1/ha/profiles/{id}/start-array
///
/// Update only the start array in the TOML configuration, preserving all other settings
#[utoipa::path(
    put,
    path = "/api/v1/ha/profiles/{id}/start-array",
    tag = "HA",
    params(
        ("id" = String, Path, description = "Profile ID or name")
    ),
    request_body = UpdateStartArrayRequest,
    responses(
        (status = 200, description = "Start array updated successfully", body = TomlContentResponse),
        (status = 404, description = "TOML file not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn update_start_array(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
    Json(request): Json<UpdateStartArrayRequest>,
) -> AppResult<Json<UpdateStartArrayResponse>> {
    // First, get the profile details to find configured nodes
    // Use the same logic as get_profile endpoint
    let profile_details = match super::list::fetch_profile_details(state.clone(), id.clone()).await
    {
        Ok(details) => details,
        Err(e) => {
            tracing::warn!(
                "Failed to fetch profile details for '{}': {}, will try to sync anyway",
                id,
                e
            );
            // Continue anyway - we'll try to determine nodes differently
            return Err(e);
        }
    };

    let configured_hostnames: Vec<String> = profile_details
        .configured_nodes
        .iter()
        .map(|n| n.hostname.clone())
        .collect();

    // Build a map of hostname -> IP from configured_nodes
    let configured_node_map: std::collections::HashMap<String, String> = profile_details
        .configured_nodes
        .iter()
        .map(|n| (n.hostname.clone(), n.ip.clone()))
        .collect();

    tracing::info!(
        "Profile '{}' has {} configured nodes: {:?}",
        id,
        configured_hostnames.len(),
        configured_hostnames
    );

    tracing::info!("Configured nodes with IPs: {:?}", configured_node_map);

    let config_path = state.reactor_config_path(&id);

    // Check if the TOML file exists
    if !state
        .controller_file_exists(&config_path)
        .await
        .unwrap_or(false)
    {
        return Err(AppError::NotFound(format!(
            "TOML configuration file not found for profile '{}': {}",
            id, config_path
        )));
    }

    // Read the existing TOML content
    let content = state.read_controller_file(&config_path).await?;

    // Parse using toml_edit to preserve formatting and comments
    let mut doc: toml_edit::DocumentMut = content.parse().map_err(|e| {
        AppError::Validation(format!(
            "Failed to parse TOML file '{}': {}",
            config_path, e
        ))
    })?;

    // Find and update the start array
    // The TOML structure is: [[promoter]] -> [promoter.resources.{resource_name}] -> start
    let mut found = false;

    // Use direct indexing to access the start array
    // Format: doc["promoter"][0]["resources"][{resource_name}]["start"]
    if let Some(promoters) = doc.get_mut("promoter") {
        if let Some(promoter_array) = promoters.as_array_of_tables_mut() {
            // Get the first (and usually only) promoter
            if let Some(first_promoter) = promoter_array.iter_mut().next() {
                if let Some(resources) = first_promoter.get_mut("resources") {
                    if let Some(resources_table) = resources.as_table_like_mut() {
                        // Iterate through each resource
                        for (_res_name, resource_item) in resources_table.iter_mut() {
                            if let Some(resource_table) = resource_item.as_table_like_mut() {
                                if resource_table.contains_key("start") {
                                    // Create new array from request
                                    let mut new_array = toml_edit::Array::new();
                                    for item in &request.start {
                                        new_array.push(item.as_str());
                                    }

                                    // Insert the new array
                                    resource_table
                                        .insert("start", toml_edit::Item::Value(new_array.into()));
                                    found = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !found {
        return Err(AppError::NotFound(
            "Could not find 'start' array in TOML configuration".to_string(),
        ));
    }

    // Get the updated TOML content
    let updated_content = doc.to_string();

    // Write back to file
    state.write_controller_file(&config_path, &updated_content).await?;

    tracing::info!(
        "Updated start array in TOML configuration for profile '{}': {}",
        id,
        config_path
    );

    // Reload systemd and drbd-reactor on local node
    let systemd = SystemdController::new().await;
    if let Ok(sys) = systemd {
        let _ = sys.daemon_reload().await;
        let _ = sys.reload("drbd-reactor.service").await;
    }

    // Get local hostname to identify which node is local
    let local_hostname = state.controller_hostname();
    tracing::info!("Local hostname: {}", local_hostname);

    // Get default SSH settings from config
    let default_ssh_user = &state.config.ssh.default_user;
    let default_ssh_port = state.config.ssh.default_port;
    tracing::info!(
        "Default SSH settings: user={}, port={}",
        default_ssh_user,
        default_ssh_port
    );

    // Get all nodes from node_store for SSH credentials
    let all_nodes = state.node_store.get_all().unwrap_or_default();
    let credential = SshCredential::Password("ignored".to_string());

    // Sync to configured nodes only
    let mut synced_nodes = Vec::new();

    for (hostname, ip) in &configured_node_map {
        let is_local = all_nodes
            .iter()
            .find(|candidate| &candidate.hostname == hostname || &candidate.ip == ip)
            .map(|candidate| state.is_controller_node(candidate))
            .unwrap_or_else(|| hostname == &local_hostname);

        if is_local {
            tracing::info!("Skipping local node {}", hostname);
            continue;
        }

        // Try to find SSH credentials from node_store
        let node_info = all_nodes
            .iter()
            .find(|n| &n.hostname == hostname || &n.ip == ip);

        let (ssh_user, ssh_port) = if let Some(node) = node_info {
            tracing::info!(
                "Found SSH info for {} from node_store: user={}, port={}",
                hostname,
                node.ssh_user,
                node.ssh_port
            );
            (node.ssh_user.as_str(), node.ssh_port)
        } else {
            tracing::warn!(
                "Node {} not found in node_store, using config defaults: user={}, port={}",
                hostname,
                default_ssh_user,
                default_ssh_port
            );
            (default_ssh_user.as_str(), default_ssh_port)
        };

        tracing::info!(
            "Syncing TOML config to node {} ({}) as {}@{}:{}",
            hostname,
            ip,
            ssh_user,
            ip,
            ssh_port
        );

        // Write the file using SshManager's write_file method
        match state
            .ssh_manager
            .write_file(
                ip,
                ssh_port,
                ssh_user,
                &credential,
                &config_path,
                &updated_content,
            )
            .await
        {
            Ok(_) => {
                tracing::info!("✓ Successfully synced to node {}", hostname);
                synced_nodes.push(hostname.clone());

                // Reload drbd-reactor on remote node
                let remote_systemd = RemoteSystemdController::new(state.ssh_manager.clone());

                let _ = remote_systemd
                    .daemon_reload(ip, ssh_port, ssh_user, &credential)
                    .await;

                let _ = remote_systemd
                    .reload("drbd-reactor.service", ssh_port, ip, &credential, ssh_user)
                    .await;
            }
            Err(e) => {
                tracing::error!("✗ Failed to sync to node {}: {}", hostname, e);
            }
        }
    }

    let total_configured = configured_hostnames.len();
    let success = !synced_nodes.is_empty();

    let message = if success {
        format!(
            "Configuration saved, synced to {}/{} configured node(s), and drbd-reactor reloaded",
            synced_nodes.len(),
            total_configured
        )
    } else if total_configured > 1 {
        "Configuration saved locally but no remote configured nodes to sync".to_string()
    } else {
        "Configuration saved (single-node cluster, no sync needed)".to_string()
    };

    tracing::info!(
        "Update start array for '{}': synced to {}/{} configured nodes",
        id,
        synced_nodes.len(),
        total_configured
    );

    Ok(Json(UpdateStartArrayResponse {
        profile: id,
        content: updated_content,
        path: config_path,
        synced_nodes,
        success,
        message,
    }))
}

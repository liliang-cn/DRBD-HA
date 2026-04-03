//! HA Profile list and query operations
//!
//! Handles listing profiles, fetching profile details, and status queries.

use axum::{
    extract::{Path, State},
    Json,
};
use drbd_reactor_utils::DrbdReactorClient;
use std::sync::Arc;

use crate::core::{
    drbd_cmd::DrbdCmd, run_shell_command, systemd_ctrl::SystemdController,
    ReactorConfigPaths as ConfigPaths,
};
use crate::error::AppResult;
use crate::state::AppState;

use super::types::{
    ConfigVisibility, HaProfileDetailResponse, HaProfileListResponse, ServiceStatusInfo,
};
use super::utils::{create_profile_from_toml, get_all_ha_profile_names};

/// Implementation of drbd_utils::RemoteExecutor using the backend's SSH manager
struct SshExecutor {
    ssh_manager: Arc<crate::core::ssh_manager::SshManager>,
    credential: crate::core::ssh_manager::SshCredential,
}

impl drbd_utils::RemoteExecutor for SshExecutor {
    fn execute(
        &self,
        ip: &str,
        port: u16,
        user: &str,
        command: &str,
    ) -> impl std::future::Future<Output = drbd_utils::DrbdResult<drbd_utils::CommandOutput>> + Send
    {
        let ssh_manager = self.ssh_manager.clone();
        let credential = self.credential.clone();
        let ip = ip.to_string();
        let user = user.to_string();
        let command = command.to_string();

        async move {
            ssh_manager
                .execute(&ip, port, &user, &credential, &command)
                .await
                .map(|output| drbd_utils::CommandOutput {
                    stdout: output.stdout,
                    stderr: output.stderr,
                    exit_code: output.exit_code as i32,
                })
                .map_err(|e| drbd_utils::DrbdError::Command(format!("SSH execution failed: {}", e)))
        }
    }
}

/// Resolve hostname to IP address using DNS lookup
/// Returns the first IPv4 address found, or None if resolution fails
fn resolve_hostname_to_ip(hostname: &str) -> Option<String> {
    drbd_utils::resolve_hostname_to_ip(hostname)
}

/// GET /api/v1/ha/profiles
#[utoipa::path(
    get,
    path = "/api/v1/ha/profiles",
    tag = "ha",
    summary = "List all HA profiles",
    responses(
        (status = 200, description = "List of HA profiles", body = HaProfileListResponse)
    )
)]
pub async fn list_profiles(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<HaProfileListResponse>> {
    let profile_names = get_all_ha_profile_names(&state).await?;
    let mut profiles = Vec::new();
    let reactor_dir = ConfigPaths::REACTOR_CONF_DIR;

    for name in &profile_names {
        // Try to read from .toml (enabled) or .toml.disabled (disabled)
        let config_path = ConfigPaths::promoter_path(name);
        let disabled_path = format!("{}/{}.toml.disabled", reactor_dir, name);

        let (content, is_locally_disabled) =
            if let Ok(content) = state.read_controller_file(&config_path).await {
                (Some(content), false)
            } else if let Ok(content) = state.read_controller_file(&disabled_path).await {
                (Some(content), true)
            } else {
                (None, false)
            };

        if let Some(content_str) = content {
            if let Some(mut profile) = create_profile_from_toml(name, &content_str) {
                // Check if profile is enabled on ANY node
                // If local node has .toml, it's definitely enabled
                // If local has .toml.disabled, check other nodes via SSH
                let (is_enabled_on_any_node, active_node_from_remote) = if !is_locally_disabled {
                    (true, None)
                } else {
                    // Local is disabled, check other nodes via SSH
                    check_if_enabled_on_other_nodes(&state, name, &content_str).await
                };

                if is_enabled_on_any_node {
                    // Profile is enabled on at least one node
                    if let Some(active_node) = active_node_from_remote {
                        // We found the active node from remote check
                        profile.status = crate::models::HaProfileStatus::Active;
                        profile.active_node = Some(active_node);
                    } else if !is_locally_disabled {
                        // Local enabled, use local reactor status
                        match DrbdReactorClient::status(Some(name), None).await {
                            Ok((statuses, _)) => {
                                if let Some(status) = statuses.first() {
                                    if status.is_active {
                                        profile.status = crate::models::HaProfileStatus::Active;
                                        profile.active_node = status.active_node.clone();
                                    } else {
                                        profile.status = crate::models::HaProfileStatus::Standby;
                                        profile.active_node = None;
                                    }
                                } else {
                                    profile.status = crate::models::HaProfileStatus::Standby;
                                }
                            }
                            Err(_) => {
                                profile.status = crate::models::HaProfileStatus::Standby;
                            }
                        }
                    } else {
                        // Remote enabled but we don't know which is active
                        profile.status = crate::models::HaProfileStatus::Standby;
                        profile.active_node = None;
                    }
                } else {
                    // Profile is disabled on all nodes
                    profile.status = crate::models::HaProfileStatus::Disabled;
                    profile.active_node = None;
                }

                profiles.push(profile);
            }
        }
    }

    Ok(Json(HaProfileListResponse { profiles }))
}

/// Helper function to check if a profile is enabled on any other node
/// Returns (is_enabled_on_any_node, active_node_from_remote)
async fn check_if_enabled_on_other_nodes(
    state: &Arc<AppState>,
    profile_name: &str,
    _local_content: &str,
) -> (bool, Option<String>) {
    use crate::core::SshCredential;

    // Get all nodes
    let nodes = match state.node_store.get_all() {
        Ok(n) => n,
        Err(_) => return (false, None),
    };

    for node in nodes {
        if state.is_controller_node(&node) {
            continue;
        }

        let reactor_dir = config_gen::ConfigPaths::REACTOR_CONF_DIR;

        // Check if .toml file exists on remote node (not .toml.disabled)
        let check_cmd = format!(
            "test -f {}/{}.toml && echo exists",
            reactor_dir, profile_name
        );
        let credential = SshCredential::Password("ignored".to_string());

        let has_toml = match state
            .ssh_manager
            .execute(
                &node.ip,
                node.ssh_port,
                &node.ssh_user,
                &credential,
                &check_cmd,
            )
            .await
        {
            Ok(output) => output.stdout.contains("exists"),
            Err(_) => false,
        };

        if has_toml {
            let status_cmd = format!(
                "sudo drbd-reactorctl status --json {} 2>/dev/null",
                profile_name
            );

            match state
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &status_cmd,
                )
                .await
            {
                Ok(output) => {
                    let statuses = drbd_reactor_utils::parser::parse_reactor_status(
                        &output.stdout,
                        Some(profile_name),
                    );
                    if let Some(status) = statuses.first() {
                        return (true, status.active_node.clone());
                    }
                    return (true, None);
                }
                Err(_) => return (true, None),
            }
        }
    }

    (false, None)
}

/// GET /api/v1/ha/profiles/:id
#[utoipa::path(
    get,
    path = "/api/v1/ha/profiles/{id}",
    tag = "ha",
    params(
        ("id" = String, Path, description = "Profile ID or name")
    ),
    responses(
        (status = 200, description = "HA profile details", body = HaProfileDetailResponse),
        (status = 404, description = "Profile not found")
    )
)]
pub async fn get_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<HaProfileDetailResponse>> {
    fetch_profile_details(state, id).await
}

/// Helper function to fetch detailed profile status
pub async fn fetch_profile_details(
    state: Arc<AppState>,
    id_or_name: String,
) -> AppResult<Json<HaProfileDetailResponse>> {
    use crate::core::SshCredential;
    use crate::models::HaProfileStatus;

    // Load profile from toml file (try both .toml and .toml.disabled)
    let config_path = ConfigPaths::promoter_path(&id_or_name);
    let reactor_dir = ConfigPaths::REACTOR_CONF_DIR;
    let disabled_path = format!("{}/{}.toml.disabled", reactor_dir, &id_or_name);

    let (content, is_disabled) = if let Ok(content) = state.read_controller_file(&config_path).await
    {
        (content, false)
    } else if let Ok(content) = state.read_controller_file(&disabled_path).await {
        (content, true)
    } else {
        return Err(crate::error::AppError::NotFound(format!(
            "HA profile {} not found",
            id_or_name
        )));
    };

    let mut profile = create_profile_from_toml(&id_or_name, &content).ok_or_else(|| {
        crate::error::AppError::NotFound(format!("HA profile {} not found", id_or_name))
    })?;

    // Check if profile is truly disabled by trying drbd-reactorctl status --json
    // First check locally, then remote nodes if local is disabled
    let is_truly_disabled = match DrbdReactorClient::status(Some(&id_or_name), None).await {
        Ok((statuses, _)) => {
            if statuses.is_empty() {
                // No valid status locally - check if local is disabled
                if is_disabled {
                    // Local is disabled, check remote nodes via SSH
                    // Check if any node has .toml file (not .toml.disabled)
                    match state.node_store.get_all() {
                        Ok(nodes) => {
                            let reactor_dir = config_gen::ConfigPaths::REACTOR_CONF_DIR;

                            let mut enabled_on_remote = false;
                            for node in nodes {
                                if state.is_controller_node(&node) {
                                    continue;
                                }

                                // Check if this node has .toml file (not .toml.disabled)
                                let check_cmd = format!(
                                    "sudo test -f {}/{}.toml && echo enabled",
                                    reactor_dir, id_or_name
                                );
                                let credential =
                                    crate::core::SshCredential::Password("ignored".to_string());

                                match state
                                    .ssh_manager
                                    .execute(
                                        &node.ip,
                                        node.ssh_port,
                                        &node.ssh_user,
                                        &credential,
                                        &check_cmd,
                                    )
                                    .await
                                {
                                    Ok(output) => {
                                        if output.stdout.trim() == "enabled" {
                                            tracing::debug!(
                                                "Profile {} is enabled on remote node {}",
                                                id_or_name,
                                                node.hostname
                                            );
                                            enabled_on_remote = true;
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            "Failed to check remote node {}: {}",
                                            node.hostname,
                                            e
                                        );
                                    }
                                }
                            }

                            !enabled_on_remote // If no remote node has it enabled, it's truly disabled
                        }
                        Err(_) => is_disabled, // If we can't check remote nodes, use local status
                    }
                } else {
                    // Local is not disabled
                    false
                }
            } else {
                // Has valid status - enabled on at least one node
                false
            }
        }
        Err(_) => {
            // Reactor status failed - check if local is disabled
            is_disabled
        }
    };

    if is_truly_disabled {
        profile.status = HaProfileStatus::Disabled;
        profile.active_node = None;

        // Still fetch configured_nodes even for disabled profiles
        // Try to get nodes from res file
        let res_file_path = state.drbd_resource_path(&profile.resource_name);
        let res_file_nodes = match state.read_controller_file(&res_file_path).await {
            Ok(content) => {
                let parsed = drbd_utils::parse_res_file_for_nodes(&content);
                tracing::debug!(
                    "Parsed res file {}: {} nodes found",
                    res_file_path,
                    parsed.len()
                );
                Some(parsed)
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to read res file {}: {}, trying LINSTOR path",
                    res_file_path,
                    e
                );
                // Try LINSTOR path
                let linstor_path = format!("/var/lib/linstor.d/{}.res", profile.resource_name);
                match state.read_controller_file(&linstor_path).await {
                    Ok(content) => {
                        let parsed = drbd_utils::parse_res_file_for_nodes(&content);
                        tracing::debug!(
                            "Parsed LINSTOR res file {}: {} nodes found",
                            linstor_path,
                            parsed.len()
                        );
                        Some(parsed)
                    }
                    Err(linstor_err) => {
                        tracing::warn!(
                            "Failed to read LINSTOR res file {}: {}",
                            linstor_path,
                            linstor_err
                        );
                        None
                    }
                }
            }
        };

        let node_infos = if let Some(nodes) = res_file_nodes {
            nodes
                .into_iter()
                .map(|(hostname, ip)| super::types::NodeConfigInfo {
                    hostname,
                    ip,
                    peer_role: None,
                    disabled: None,
                })
                .collect()
        } else {
            // Fallback: get from node_store
            match state.node_store.get_all() {
                Ok(ns) => ns
                    .into_iter()
                    .map(|n| super::types::NodeConfigInfo {
                        hostname: n.hostname,
                        ip: n.ip,
                        peer_role: None,
                        disabled: None,
                    })
                    .collect(),
                Err(_) => Vec::new(),
            }
        };

        let configured_nodes = check_nodes_disabled_status(&state, &id_or_name, node_infos).await;

        // Read DRBD config file content for disabled profiles too
        let drbd_config_raw = match state.read_controller_file(&res_file_path).await {
            Ok(content) => Some(content),
            Err(_) => {
                // Try LINSTOR path as fallback
                let linstor_path = format!("/var/lib/linstor.d/{}.res", profile.resource_name);
                state.read_controller_file(&linstor_path).await.ok()
            }
        };

        // Read drbd-reactor promoter config file content for disabled profiles
        let reactor_dir = config_gen::ConfigPaths::REACTOR_CONF_DIR;
        let promoter_config_path = format!("{}/{}.toml", reactor_dir, id_or_name);
        let promoter_config_raw = state.read_controller_file(&promoter_config_path).await.ok();

        // Return for disabled profiles with configured_nodes
        return Ok(Json(HaProfileDetailResponse {
            profile,
            status: HaProfileStatus::Disabled,
            active_node: None,
            mount_point: None,
            drbd: None,
            drbd_device: None,
            service_statuses: Vec::new(),
            vip_active: Some(false),
            config: crate::api::ha::types::ConfigVisibility {
                promoter_config_exists: true,
                promoter_config_path: format!("/etc/drbd-reactor.d/{}.toml.disabled", id_or_name),
                reactor_running: true,
            },
            reactor_status_raw: Some(String::new()),
            drbd_config_raw,
            promoter_config_raw,
            configured_nodes,
        }));
    }

    // Step 1: Get DRBD status to determine which node is Primary (active)
    // Use drbdsetup status --json for complete status including connection state
    let local_drbd_status = {
        let cmd = format!(
            "{} 2>/dev/null",
            DrbdCmd::resource_status_cmd(&profile.resource_name)?
        );
        let output = run_shell_command(
            &cmd,
            &format!("Get local DRBD status for {}", profile.resource_name),
        )
        .await?;

        tracing::debug!(
            "drbdsetup status --json exit_code: {}, stdout: {}",
            output.exit_code,
            if output.stdout.is_empty() {
                "(empty)".to_string()
            } else {
                output.stdout.clone()
            }
        );

        if output.success() && !output.stdout.is_empty() {
            // Parse JSON output from drbdsetup
            match drbd_utils::parse_drbd_status(&output.stdout) {
                Ok(resources) => {
                    // Find our resource
                    if let Some(resource) =
                        resources.iter().find(|r| r.name == profile.resource_name)
                    {
                        Some(drbd_utils::convert_resource_status(resource))
                    } else {
                        tracing::warn!(
                            "Resource '{}' not found in drbdsetup status output",
                            profile.resource_name
                        );
                        None
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to parse drbdsetup JSON output: {}", e);
                    None
                }
            }
        } else {
            tracing::warn!(
                "drbdsetup status failed or empty for resource '{}', exit_code: {}",
                profile.resource_name,
                output.exit_code
            );
            None
        }
    };

    // Determine active node from local DRBD status
    let local_hostname = state.controller_hostname();

    // Parse res file to get hostname -> IP mapping
    let res_file_path = state.drbd_resource_path(&profile.resource_name);
    let res_file_nodes = match state.read_controller_file(&res_file_path).await {
        Ok(content) => {
            let parsed = drbd_utils::parse_res_file_for_nodes(&content);
            tracing::debug!(
                "Parsed res file {}: {} nodes found",
                res_file_path,
                parsed.len()
            );
            parsed
        }
        Err(e) => {
            tracing::warn!(
                "Failed to read res file {}: {}, trying LINSTOR path",
                res_file_path,
                e
            );
            // Try LINSTOR path
            let linstor_path = format!("/var/lib/linstor.d/{}.res", profile.resource_name);
            match state.read_controller_file(&linstor_path).await {
                Ok(content) => {
                    let parsed = drbd_utils::parse_res_file_for_nodes(&content);
                    tracing::debug!(
                        "Parsed LINSTOR res file {}: {} nodes found",
                        linstor_path,
                        parsed.len()
                    );
                    parsed
                }
                Err(linstor_err) => {
                    tracing::warn!(
                        "Failed to read LINSTOR res file {}: {}",
                        linstor_path,
                        linstor_err
                    );
                    std::collections::HashMap::new()
                }
            }
        }
    };

    // Find the Primary (active) node from DRBD status
    // If local is Primary, active = local. If local is Secondary, find Primary in peers.
    let active_node_from_drbd = if let Some(ref status) = local_drbd_status {
        if status.role == "Primary" {
            Some(local_hostname.clone())
        } else {
            // Local is Secondary, find Primary in peers
            status
                .peers
                .iter()
                .find(|p| p.role == "Primary")
                .map(|p| p.name.clone())
        }
    } else {
        // DRBD status not available, try drbd-reactor as fallback
        None
    };

    // Prepare remote query helper for SSH execution
    let credential = SshCredential::Password("ignored".to_string());
    let ssh_executor = SshExecutor {
        ssh_manager: state.ssh_manager.clone(),
        credential,
    };
    let ssh_port = 22;
    let ssh_user = "root";
    let remote_query = drbd_utils::RemoteDrbdQuery::new(ssh_executor, ssh_port, ssh_user);

    // Get local hostname
    let local_hostname = state.controller_hostname();

    // Step 2: Execute drbdadm status and drbd-reactorctl status --json on the active node
    // Track whether DRBD status is from local or remote node
    let (drbd, active_node, reactor_status_raw, drbd_from_local) =
        if let Some(ref active_hostname) = active_node_from_drbd {
            // Check if active node is local by comparing hostname
            if active_hostname == &local_hostname {
                // Active node is local, execute locally
                tracing::info!(
                    "Active node is local, getting status locally for {}",
                    profile.resource_name
                );

                let reactor_status = DrbdReactorClient::status(Some(&profile.name), None).await;
                let reactor_raw = reactor_status.as_ref().map(|(_, raw)| raw.clone()).ok();
                let active_from_reactor = reactor_status
                    .as_ref()
                    .ok()
                    .and_then(|(statuses, _)| statuses.first())
                    .and_then(|s| s.active_node.clone());

                (
                    local_drbd_status,
                    active_from_reactor.or_else(|| Some(active_hostname.clone())),
                    reactor_raw,
                    true,
                )
            } else {
                // Active node is remote - SSH to execute commands there
                tracing::info!(
                    "Active node is remote {}, getting status via SSH for {}",
                    active_hostname,
                    profile.resource_name
                );

                // Get IP from res file
                let remote_ip = if let Some(ip) = res_file_nodes.get(active_hostname) {
                    ip.clone()
                } else {
                    // Fallback: try DNS resolution
                    tracing::warn!(
                        "IP not found in res file for {}, trying DNS",
                        active_hostname
                    );
                    resolve_hostname_to_ip(active_hostname).unwrap_or_else(|| {
                        tracing::error!("Cannot resolve IP for {}", active_hostname);
                        local_hostname.clone()
                    })
                };

                // If IP resolution failed and we're using local hostname, skip SSH
                if remote_ip == local_hostname || remote_ip == "127.0.0.1" {
                    let reactor_status = DrbdReactorClient::status(Some(&profile.name), None).await;
                    let reactor_raw = reactor_status.as_ref().map(|(_, raw)| raw.clone()).ok();
                    let active_from_reactor = reactor_status
                        .as_ref()
                        .ok()
                        .and_then(|(statuses, _)| statuses.first())
                        .and_then(|s| s.active_node.clone());
                    (
                        local_drbd_status,
                        active_from_reactor.or_else(|| Some(active_hostname.clone())),
                        reactor_raw,
                        true,
                    )
                } else {
                    // Use RemoteDrbdQuery to get DRBD and reactor status from remote node
                    let remote_drbd = remote_query
                        .get_resource_status(&remote_ip, &profile.resource_name)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!(
                                "Failed to get DRBD status from {} via SSH: {}",
                                active_hostname,
                                e
                            );
                            None
                        });

                    let reactor_result = remote_query
                        .get_reactor_status(&remote_ip, &profile.name)
                        .await;

                    // If remote reactor status fails, try to get it locally
                    let reactor_raw = match reactor_result {
                        Ok(Some(output)) if !output.is_empty() => Some(output),
                        _ => {
                            // Remote failed or returned empty, try local reactor status
                            tracing::warn!(
                            "Remote reactor status failed or empty, trying local reactor status"
                        );
                            match DrbdReactorClient::status(Some(&profile.name), None).await {
                                Ok((_, raw_output)) => Some(raw_output),
                                Err(e) => {
                                    tracing::warn!("Local reactor status also failed: {}", e);
                                    None
                                }
                            }
                        }
                    };

                    // Try to get active node from reactor output
                    let active_from_reactor = reactor_raw
                        .as_ref()
                        .and_then(|raw| {
                            drbd_reactor_utils::parser::parse_reactor_status(
                                raw,
                                Some(&profile.name),
                            )
                            .into_iter()
                            .next()
                            .and_then(|status| status.active_node)
                        })
                        .or_else(|| Some(active_hostname.clone()));

                    (remote_drbd, active_from_reactor, reactor_raw, false)
                }
            }
        } else {
            // No active node from DRBD, use local status
            tracing::info!("No active node determined from DRBD, using local status");
            let reactor_status = DrbdReactorClient::status(Some(&profile.name), None).await;

            // Extract raw output and active node from reactor status
            // Always try to get raw output, even if parsing fails
            let reactor_raw = match &reactor_status {
                Ok((_, raw)) => Some(raw.clone()),
                Err(_) => {
                    // Fallback: try to get raw output directly from command
                    let cmd = format!(
                        "sudo drbd-reactorctl status --json {} 2>/dev/null",
                        profile.name
                    );
                    match crate::core::run_shell_command(&cmd, "Get drbd-reactor status").await {
                        Ok(output) => Some(output.stdout),
                        Err(_) => None,
                    }
                }
            };

            // Try to extract active node from raw JSON output if statuses parsing failed
            let active_from_reactor = if let Ok((statuses, _)) = &reactor_status {
                statuses.first().and_then(|s| s.active_node.clone())
            } else {
                reactor_raw.as_ref().and_then(|raw| {
                    drbd_reactor_utils::parser::parse_reactor_status(raw, Some(&profile.name))
                        .into_iter()
                        .next()
                        .and_then(|status| status.active_node)
                })
            };

            (local_drbd_status, active_from_reactor, reactor_raw, true)
        };

    let drbd_role = drbd.as_ref().map(|d| d.role.as_str());

    // Initialize service statuses from configured services
    let mut service_statuses: Vec<ServiceStatusInfo> = profile
        .promoter
        .services
        .iter()
        .map(|name| ServiceStatusInfo {
            name: name.clone(),
            active: false,
            state: "inactive".to_string(),
            enabled: false,
            active_since: None,
        })
        .collect();

    if let Some(raw) = &reactor_status_raw {
        let parsed_services = drbd_reactor_utils::parser::parse_reactor_services(raw);
        for parsed in parsed_services {
            // Update existing or add new
            if let Some(existing) = service_statuses.iter_mut().find(|s| s.name == parsed.name) {
                existing.active = parsed.active;
                existing.state = parsed.state;
            } else {
                service_statuses.push(ServiceStatusInfo {
                    name: parsed.name,
                    active: parsed.active,
                    state: parsed.state,
                    enabled: false,
                    active_since: None,
                });
            }
        }

        // Query systemd for enabled status of each service
        // We need to query on the active node where drbd-reactor is managing services
        if let Some(active_node_name) = &active_node {
            // Check if active node is local by comparing hostname
            if active_node_name == &local_hostname {
                // Active node is local, query local systemd
                let systemd = SystemdController::new().await?;
                for service_info in &mut service_statuses {
                    if service_info.name.starts_with("ocf.rs@") {
                        continue;
                    }
                    if let Ok(status) = systemd.status(&service_info.name).await {
                        service_info.enabled = status.is_enabled();
                    }
                }
            } else {
                // Active node is remote, use RemoteDrbdQuery to query systemd there
                // Get IP from res file
                let remote_ip = if let Some(ip) = res_file_nodes.get(active_node_name) {
                    ip.clone()
                } else {
                    // Fallback: try DNS resolution
                    tracing::warn!(
                        "IP not found in res file for {}, trying DNS",
                        active_node_name
                    );
                    resolve_hostname_to_ip(active_node_name).unwrap_or_else(|| {
                        tracing::error!("Cannot resolve IP for {}", active_node_name);
                        local_hostname.clone()
                    })
                };

                // Only query if we have a valid remote IP
                if remote_ip != local_hostname && remote_ip != "127.0.0.1" {
                    for service_info in &mut service_statuses {
                        if service_info.name.starts_with("ocf.rs@") {
                            continue;
                        }
                        // Use RemoteDrbdQuery to check if service is enabled on remote node
                        match remote_query
                            .is_service_enabled(&remote_ip, &service_info.name)
                            .await
                        {
                            Ok(enabled) => {
                                service_info.enabled = enabled;
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to check service {} status on {}: {}",
                                    service_info.name,
                                    active_node_name,
                                    e
                                );
                                service_info.enabled = false;
                            }
                        }
                    }
                }
            }
        }
    }

    let mount_point = service_statuses
        .iter()
        .find(|s| s.name.ends_with(".mount"))
        .and_then(|s| {
            let name = s.name.strip_suffix(".mount")?;
            Some(format!("/{}", name.replace('-', "/")))
        });

    let promoter_config_path = ConfigPaths::promoter_path(&profile.name);
    let promoter_config_exists = state
        .controller_file_exists(&promoter_config_path)
        .await
        .unwrap_or(false);
    let systemd = SystemdController::new().await?;
    let reactor_service_status = systemd.status("drbd-reactor.service").await?;
    let config = ConfigVisibility {
        promoter_config_exists,
        promoter_config_path: promoter_config_path.clone(),
        reactor_running: reactor_service_status.is_running(),
    };

    let vip_active = if let Some(vip) = &profile.vip {
        let mut active = false;

        if promoter_config_exists {
            if let Ok(cfg) = state.read_controller_file(&promoter_config_path).await {
                if cfg.contains("ocf:heartbeat:IPaddr2") && active_node.is_some() {
                    active = true;
                }
            }
        }

        if !active && active_node.is_some() {
            active = true;
        }

        if !active {
            // Check if VIP is active on any interface
            let cmd = format!("ip addr show | grep -q '{}/'", vip.address);
            let output =
                run_shell_command(&cmd, &format!("Check if VIP {} is active", vip.address)).await?;
            active = output.success();
        }

        Some(active)
    } else {
        let has_vip_service = service_statuses
            .iter()
            .any(|s| s.name.contains("vip") && s.name.contains("ocf.rs@service"));

        if has_vip_service {
            let vip_service_active = service_statuses
                .iter()
                .find(|s| s.name.contains("vip") && s.name.contains("ocf.rs@service"))
                .map(|s| s.active)
                .unwrap_or(false);
            Some(vip_service_active)
        } else {
            None
        }
    };

    let status = if active_node.is_some() {
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

    let mut profile_out = profile.clone();
    profile_out.status = status.clone();

    // Get DRBD device name from DRBD status (minor number)
    let drbd_device = if let Some(drbd_status) = &drbd {
        // Build device path from minor number: /dev/drbd{minor}
        drbd_status.minor.map(|m| format!("/dev/drbd{}", m))
    } else {
        None
    };

    // Build list of configured nodes from DRBD res file and DRBD status
    let configured_nodes = {
        let mut node_infos = Vec::new();
        let local_hostname = state.controller_hostname();

        // Helper to convert DRBD role to display role
        let role_to_display = |role: &str| -> String {
            match role {
                "Primary" => "Active".to_string(),
                "Secondary" => "Standby".to_string(),
                _ => role.to_string(),
            }
        };

        // Parse res file to get hostname -> IP mapping
        // Try multiple locations:
        // 1. Standard DRBD path: /etc/drbd.d/{resource}.res
        // 2. LINSTOR path: /var/lib/linstor.d/{resource}.res
        let res_file_path = state.drbd_resource_path(&profile.resource_name);
        let res_file_nodes = match state.read_controller_file(&res_file_path).await {
            Ok(content) => {
                let parsed = drbd_utils::parse_res_file_for_nodes(&content);
                tracing::debug!(
                    "Parsed res file {}: {} nodes found",
                    res_file_path,
                    parsed.len()
                );
                Some(parsed)
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to read res file {}: {}, trying LINSTOR path",
                    res_file_path,
                    e
                );
                // Try LINSTOR path
                let linstor_path = format!("/var/lib/linstor.d/{}.res", profile.resource_name);
                match state.read_controller_file(&linstor_path).await {
                    Ok(content) => {
                        let parsed = drbd_utils::parse_res_file_for_nodes(&content);
                        tracing::debug!(
                            "Parsed LINSTOR res file {}: {} nodes found",
                            linstor_path,
                            parsed.len()
                        );
                        Some(parsed)
                    }
                    Err(linstor_err) => {
                        tracing::warn!(
                            "Failed to read LINSTOR res file {}: {}",
                            linstor_path,
                            linstor_err
                        );
                        None
                    }
                }
            }
        };

        if let Some(drbd_status) = &drbd {
            if drbd_from_local {
                // DRBD status from local node: add local + peers
                // Add local node (res file might not have local entry if we're parsing from active node)
                let local_ip = res_file_nodes
                    .as_ref()
                    .and_then(|map| map.get(&local_hostname).cloned())
                    .or_else(|| {
                        // Fallback: try DNS resolution
                        let resolved = resolve_hostname_to_ip(&local_hostname);
                        if resolved.is_some() {
                            tracing::debug!("Resolved {} via DNS", local_hostname);
                        }
                        resolved
                    });

                node_infos.push(super::types::NodeConfigInfo {
                    hostname: local_hostname.clone(),
                    ip: local_ip.unwrap_or_else(|| "127.0.0.1".to_string()),
                    peer_role: Some(role_to_display(&drbd_status.role)),
                    disabled: None,
                });

                // Add peer nodes from DRBD
                for peer in &drbd_status.peers {
                    let peer_ip = res_file_nodes
                        .as_ref()
                        .and_then(|map| map.get(&peer.name).cloned())
                        .or_else(|| {
                            // Fallback: try DNS resolution
                            let resolved = resolve_hostname_to_ip(&peer.name);
                            if resolved.is_some() {
                                tracing::debug!("Resolved {} via DNS", peer.name);
                            }
                            resolved
                        })
                        .unwrap_or_else(|| {
                            tracing::warn!("Could not resolve IP for {}", peer.name);
                            "Unknown".to_string()
                        });

                    node_infos.push(super::types::NodeConfigInfo {
                        hostname: peer.name.clone(),
                        ip: peer_ip,
                        peer_role: Some(role_to_display(&peer.role)),
                        disabled: None,
                    });
                }
            } else {
                // DRBD status from remote node: use all nodes from DRBD (local is in peers)
                // drbd_status.role is the remote node's role (Primary/Active)

                // Add the remote node (active node) first
                if let Some(active_hostname) = active_node.as_deref() {
                    let active_ip = res_file_nodes
                        .as_ref()
                        .and_then(|map| map.get(active_hostname).cloned())
                        .or_else(|| {
                            // Fallback: try DNS resolution
                            let resolved = resolve_hostname_to_ip(active_hostname);
                            if resolved.is_some() {
                                tracing::debug!("Resolved {} via DNS", active_hostname);
                            }
                            resolved
                        })
                        .unwrap_or_else(|| {
                            tracing::warn!("Could not resolve IP for {}", active_hostname);
                            "Unknown".to_string()
                        });

                    node_infos.push(super::types::NodeConfigInfo {
                        hostname: active_hostname.to_string(),
                        ip: active_ip,
                        peer_role: Some(role_to_display(&drbd_status.role)),
                        disabled: None,
                    });

                    // Add all peers from DRBD (this includes the local node and other peers)
                    for peer in &drbd_status.peers {
                        let peer_ip = res_file_nodes
                            .as_ref()
                            .and_then(|map| map.get(&peer.name).cloned())
                            .or_else(|| {
                                // Fallback: try DNS resolution
                                let resolved = resolve_hostname_to_ip(&peer.name);
                                if resolved.is_some() {
                                    tracing::debug!("Resolved {} via DNS", peer.name);
                                }
                                resolved
                            })
                            .unwrap_or_else(|| {
                                tracing::warn!("Could not resolve IP for {}", peer.name);
                                "Unknown".to_string()
                            });

                        node_infos.push(super::types::NodeConfigInfo {
                            hostname: peer.name.clone(),
                            ip: peer_ip,
                            peer_role: Some(role_to_display(&peer.role)),
                            disabled: None,
                        });
                    }
                }
            }
        } else {
            // No DRBD status available: add all nodes from res file
            if let Some(nodes) = res_file_nodes {
                // Add all nodes found in res file
                for (hostname, ip) in nodes.iter() {
                    node_infos.push(super::types::NodeConfigInfo {
                        hostname: hostname.clone(),
                        ip: ip.clone(),
                        peer_role: drbd_role.map(|r| role_to_display(r)),
                        disabled: None,
                    });
                }
            } else {
                // No res file either: add local node
                let local_ip = resolve_hostname_to_ip(&local_hostname)
                    .unwrap_or_else(|| "127.0.0.1".to_string());

                node_infos.push(super::types::NodeConfigInfo {
                    hostname: local_hostname.clone(),
                    ip: local_ip,
                    peer_role: drbd_role.map(|r| role_to_display(r)),
                    disabled: None,
                });
            }
        }

        // Check disabled status for each node before returning
        check_nodes_disabled_status(&state, &id_or_name, node_infos).await
    };

    // Read DRBD config file content for preview
    let drbd_config_raw = match state.read_controller_file(&res_file_path).await {
        Ok(content) => Some(content),
        Err(_) => {
            // Try LINSTOR path as fallback
            let linstor_path = format!("/var/lib/linstor.d/{}.res", profile.resource_name);
            state.read_controller_file(&linstor_path).await.ok()
        }
    };

    // Read drbd-reactor promoter config file content
    let reactor_dir = config_gen::ConfigPaths::REACTOR_CONF_DIR;
    let promoter_config_path = format!("{}/{}.toml", reactor_dir, profile.name);
    let promoter_config_raw = state.read_controller_file(&promoter_config_path).await.ok();

    Ok(Json(HaProfileDetailResponse {
        profile: profile_out,
        status,
        active_node,
        mount_point,
        drbd,
        drbd_device,
        service_statuses,
        vip_active,
        config,
        reactor_status_raw,
        drbd_config_raw,
        promoter_config_raw,
        configured_nodes,
    }))
}

/// Check disabled status for each node by SSH
/// Returns a new Vec<NodeConfigInfo> with disabled field populated
async fn check_nodes_disabled_status(
    state: &Arc<AppState>,
    profile_name: &str,
    nodes: Vec<super::types::NodeConfigInfo>,
) -> Vec<super::types::NodeConfigInfo> {
    use crate::core::SshCredential;

    let reactor_dir = config_gen::ConfigPaths::REACTOR_CONF_DIR;

    // Get full node info from store for SSH credentials
    let all_nodes = match state.node_store.get_all() {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("Failed to get nodes from store: {}", e);
            return nodes; // Return without disabled info
        }
    };

    let mut nodes_with_disabled = Vec::new();

    for node in nodes {
        let is_local = node
            .hostname
            .as_str()
            .eq(state.controller_hostname().as_str())
            || all_nodes
                .iter()
                .find(|candidate| candidate.hostname == node.hostname || candidate.ip == node.ip)
                .map(|candidate| state.is_controller_node(candidate))
                .unwrap_or(false);

        let disabled = if is_local {
            let disabled_path = format!("{}/{}.toml.disabled", reactor_dir, profile_name);
            state
                .controller_file_exists(&disabled_path)
                .await
                .unwrap_or(false)
        } else {
            // Find full node info for SSH credentials
            let full_node = all_nodes
                .iter()
                .find(|n| n.hostname == node.hostname || n.ip == node.ip);

            if let Some(target_node) = full_node {
                let check_cmd = format!(
                    "sudo test -f {}/{}.toml.disabled && echo disabled",
                    reactor_dir, profile_name
                );
                let credential = SshCredential::Password("ignored".to_string());

                match state
                    .ssh_manager
                    .execute(
                        &target_node.ip,
                        target_node.ssh_port,
                        &target_node.ssh_user,
                        &credential,
                        &check_cmd,
                    )
                    .await
                {
                    Ok(output) => {
                        let result = output.stdout.trim() == "disabled";
                        tracing::debug!(
                            "Checked disabled status for {} ({}): {}",
                            node.hostname,
                            node.ip,
                            result
                        );
                        result
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to check disabled status for {}: {}",
                            node.hostname,
                            e
                        );
                        false
                    }
                }
            } else {
                tracing::warn!("Could not find full node info for {}", node.hostname);
                false
            }
        };

        nodes_with_disabled.push(super::types::NodeConfigInfo {
            hostname: node.hostname,
            ip: node.ip,
            peer_role: node.peer_role,
            disabled: Some(disabled),
        });
    }

    nodes_with_disabled
}

/// GET /api/v1/ha/profiles/:id/status
#[utoipa::path(
    get,
    path = "/api/v1/ha/profiles/{id}/status",
    tag = "ha",
    params(
        ("id" = String, Path, description = "Profile ID or name")
    ),
    responses(
        (status = 200, description = "HA profile status", body = HaProfileDetailResponse),
        (status = 404, description = "Profile not found")
    )
)]
pub async fn get_profile_status(
    State(state): State<Arc<AppState>>,
    Path(id_or_name): Path<String>,
) -> AppResult<Json<HaProfileDetailResponse>> {
    fetch_profile_details(state, id_or_name).await
}

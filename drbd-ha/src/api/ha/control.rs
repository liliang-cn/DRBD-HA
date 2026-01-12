use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::core::{run_shell_command, systemd_ctrl::SystemdController};
use crate::error::{AppError, AppResult};
use crate::state::{AppState, NotificationLevel};
use crate::api::ha::utils::create_profile_from_toml;

use super::list::get_profile_status;
use super::reactor::reload_reactor;
use super::types::{HaProfileDetailResponse, ReactorReloadRequest};

/// POST /api/v1/ha/profiles/:id/activate
#[utoipa::path(
    post,
    path = "/api/v1/ha/profiles/{id}/activate",
    tag = "ha",
    params(
        ("id" = String, Path, description = "Profile ID or name")
    ),
    responses(
        (status = 200, description = "Profile activated", body = HaProfileDetailResponse),
        (status = 404, description = "Profile not found")
    )
)]
pub async fn activate_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<HaProfileDetailResponse>> {
    tracing::info!(
        "activate_profile: Starting activation for profile id={}",
        id
    );

    // Load profile from toml file instead of database
    let config_path = crate::core::ReactorConfigPaths::promoter_path(&id);
    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|_| AppError::NotFound(format!("HA profile {} not found", id)))?;
    let profile = create_profile_from_toml(&id, &content)
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id)))?;

    tracing::info!(
        "activate_profile: Found profile '{}', resource='{}', mount='{}'",
        profile.name,
        profile.resource_name,
        profile.mount_point
    );

    let operation_id = uuid::Uuid::new_v4().to_string();
    state.send_progress(
        &operation_id,
        "activate_profile",
        Some(&profile.name),
        0,
        "Checking DRBD resource...",
        false,
        None,
    );

    let up_cmd = format!("drbdadm up {}", profile.resource_name);
    tracing::info!("activate_profile: Running '{}'", up_cmd);
    let up_output = run_shell_command(
        &up_cmd,
        &format!("Bring up DRBD resource {}", profile.resource_name),
    )
    .await?;
    tracing::info!(
        "activate_profile: up result: success={}, stdout='{}', stderr='{}'",
        up_output.success(),
        up_output.stdout.trim(),
        up_output.stderr.trim()
    );

    if !up_output.success() {
        if up_output.stderr.contains("No valid meta data") {
            tracing::info!("activate_profile: No metadata found, creating...");
            state.send_progress(
                &operation_id,
                "activate_profile",
                Some(&profile.name),
                5,
                "Creating DRBD metadata...",
                false,
                None,
            );

            let create_md_cmd = format!("drbdadm create-md --force {}", profile.resource_name);
            tracing::info!("activate_profile: Running '{}'", create_md_cmd);
            let md_output = run_shell_command(
                &create_md_cmd,
                &format!("Create DRBD metadata for {}", profile.resource_name),
            )
            .await?;
            tracing::info!(
                "activate_profile: create-md result: success={}, stderr='{}'",
                md_output.success(),
                md_output.stderr.trim()
            );

            if !md_output.success() {
                state.send_progress(
                    &operation_id,
                    "activate_profile",
                    Some(&profile.name),
                    10,
                    &format!("Failed to create metadata: {}", md_output.stderr),
                    true,
                    Some(false),
                );
                return Err(AppError::Drbd(format!(
                    "Failed to create DRBD metadata: {}",
                    md_output.stderr
                )));
            }

            tracing::info!("activate_profile: Retrying up command");
            let up_retry = run_shell_command(
                &up_cmd,
                &format!("Retry bringing up DRBD resource {}", profile.resource_name),
            )
            .await?;
            tracing::info!(
                "activate_profile: up retry result: success={}, stderr='{}'",
                up_retry.success(),
                up_retry.stderr.trim()
            );

            if !up_retry.success() && !up_retry.stderr.contains("already") {
                state.send_progress(
                    &operation_id,
                    "activate_profile",
                    Some(&profile.name),
                    10,
                    &format!("Failed to bring up resource: {}", up_retry.stderr),
                    true,
                    Some(false),
                );
                return Err(AppError::Drbd(format!(
                    "Failed to bring up DRBD resource: {}",
                    up_retry.stderr
                )));
            }
        } else if !up_output.stderr.contains("already") {
            tracing::warn!("activate_profile: drbdadm up failed: {}", up_output.stderr);
        }
    }

    let status_cmd = format!("drbdadm status {}", profile.resource_name);
    tracing::info!("activate_profile: Running '{}'", status_cmd);
    let status_output = run_shell_command(
        &status_cmd,
        &format!("Get DRBD status for {}", profile.resource_name),
    )
    .await?;
    tracing::info!("activate_profile: status output:\n{}", status_output.stdout);

    if status_output.stdout.contains("disk:Diskless") {
        tracing::info!("activate_profile: Disk is Diskless, trying to attach");
        state.send_progress(
            &operation_id,
            "activate_profile",
            Some(&profile.name),
            8,
            "Attaching disk...",
            false,
            None,
        );

        let attach_cmd = format!("drbdadm attach {}", profile.resource_name);
        tracing::info!("activate_profile: Running '{}'", attach_cmd);
        let attach_output = run_shell_command(
            &attach_cmd,
            &format!("Attach DRBD resource {}", profile.resource_name),
        )
        .await?;
        tracing::info!(
            "activate_profile: attach result: success={}, stderr='{}'",
            attach_output.success(),
            attach_output.stderr.trim()
        );

        if !attach_output.success() && attach_output.stderr.contains("No valid meta data") {
            tracing::info!("activate_profile: Attach failed due to missing metadata, creating...");
            let create_md_cmd = format!("drbdadm create-md --force {} 2>&1", profile.resource_name);
            let md_result = run_shell_command(
                &create_md_cmd,
                &format!("Create DRBD metadata for {}", profile.resource_name),
            )
            .await;
            tracing::info!("activate_profile: create-md result: {:?}", md_result);

            let retry_result = run_shell_command(
                &attach_cmd,
                &format!("Retry attaching DRBD resource {}", profile.resource_name),
            )
            .await?;
            tracing::info!("activate_profile: attach retry result: {:?}", retry_result);
        }
    }

    if status_output.stdout.contains("connection:StandAlone") {
        tracing::info!("activate_profile: Connection is StandAlone, trying to connect");
        state.send_progress(
            &operation_id,
            "activate_profile",
            Some(&profile.name),
            9,
            "Connecting to peers...",
            false,
            None,
        );
        let connect_cmd = format!("drbdadm connect {}", profile.resource_name);
        tracing::info!("activate_profile: Running '{}'", connect_cmd);
        let connect_result = run_shell_command(
            &connect_cmd,
            &format!("Connect DRBD resource {}", profile.resource_name),
        )
        .await?;
        tracing::info!("activate_profile: connect result: {:?}", connect_result);
    }

    state.send_progress(
        &operation_id,
        "activate_profile",
        Some(&profile.name),
        10,
        "Promoting DRBD resource...",
        false,
        None,
    );

    let status_cmd_check = format!("drbdadm status {}", profile.resource_name);
    let status_check_out =
        run_shell_command(&status_cmd_check, "Check role before promote").await?;
    let already_primary = status_check_out.stdout.contains("role:Primary");

    if already_primary {
        tracing::info!(
            "activate_profile: Resource {} is already Primary, skipping promotion",
            profile.resource_name
        );
    } else {
        let promote_cmd = format!("drbdadm primary {}", profile.resource_name);
        tracing::info!("activate_profile: Running '{}'", promote_cmd);
        let output = run_shell_command(
            &promote_cmd,
            &format!("Promote DRBD resource {}", profile.resource_name),
        )
        .await?;
        tracing::info!(
            "activate_profile: primary result: success={}, stderr='{}'",
            output.success(),
            output.stderr.trim()
        );

        if !output.success() {
            if output.stderr.contains("Need access to UpToDate data") {
                tracing::info!("activate_profile: Need UpToDate data, trying force promote...");
                state.send_progress(
                    &operation_id,
                    "activate_profile",
                    Some(&profile.name),
                    10,
                    "Resource not synced yet, skipping initial sync...",
                    false,
                    None,
                );

                let skip_sync_cmd = format!(
                    "drbdadm new-current-uuid --clear-bitmap {}",
                    profile.resource_name
                );
                tracing::info!("activate_profile: Running '{}'", skip_sync_cmd);
                let skip_output = run_shell_command(
                    &skip_sync_cmd,
                    &format!("Skip initial sync for {}", profile.resource_name),
                )
                .await?;
                tracing::info!(
                    "activate_profile: skip sync result: success={}, stderr='{}'",
                    skip_output.success(),
                    skip_output.stderr.trim()
                );

                let force_promote_cmd =
                    format!("drbdadm primary --force {}", profile.resource_name);
                tracing::info!("activate_profile: Running '{}'", force_promote_cmd);
                let force_output = run_shell_command(
                    &force_promote_cmd,
                    &format!("Force promote DRBD resource {}", profile.resource_name),
                )
                .await?;
                tracing::info!(
                    "activate_profile: force primary result: success={}, stderr='{}'",
                    force_output.success(),
                    force_output.stderr.trim()
                );

                if !force_output.success() {
                    state.send_progress(
                        &operation_id,
                        "activate_profile",
                        Some(&profile.name),
                        20,
                        &format!("Failed to force promote: {}", force_output.stderr),
                        true,
                        Some(false),
                    );
                    state.send_notification(
                        NotificationLevel::Error,
                        "Activation Failed",
                        &format!(
                            "Failed to promote '{}': {}",
                            profile.name, force_output.stderr
                        ),
                    );
                    return Err(AppError::Drbd(format!(
                        "Failed to promote resource: {}",
                        force_output.stderr
                    )));
                }
                tracing::info!("activate_profile: Force promote succeeded");
            } else {
                tracing::error!("activate_profile: Promote failed: {}", output.stderr);
                state.send_progress(
                    &operation_id,
                    "activate_profile",
                    Some(&profile.name),
                    20,
                    &format!("Failed to promote: {}", output.stderr),
                    true,
                    Some(false),
                );
                state.send_notification(
                    NotificationLevel::Error,
                    "Activation Failed",
                    &format!("Failed to promote '{}': {}", profile.name, output.stderr),
                );
                return Err(AppError::Drbd(format!(
                    "Failed to promote resource: {}",
                    output.stderr
                )));
            }
        }

        tracing::info!("activate_profile: DRBD promoted to Primary successfully");
    }

    // Validate DRBD status and wait for connections to be established
    state.send_progress(
        &operation_id,
        "activate_profile",
        Some(&profile.name),
        15,
        "Validating DRBD status and connections...",
        false,
        None,
    );

    let mut drbd_validated = false;
    let mut validation_attempts = 0;
    const MAX_VALIDATION_ATTEMPTS: u32 = 30; // 30 seconds with 1-second intervals

    while !drbd_validated && validation_attempts < MAX_VALIDATION_ATTEMPTS {
        validation_attempts += 1;

        // Get current DRBD status
        let status_cmd = format!("drbdadm status {}", profile.resource_name);
        let status_output = run_shell_command(
            &status_cmd,
            &format!("Check DRBD status for validation (attempt {})", validation_attempts),
        )
        .await?;

        tracing::info!(
            "activate_profile: DRBD validation attempt {}: {}",
            validation_attempts,
            status_output.stdout.trim()
        );

        // Parse DRBD status to check if it's ready
        let status_str = status_output.stdout;

        // Check 1: Resource should be Primary
        let is_primary = status_str.contains("role:Primary");

        // Check 2: Disk should be UpToDate
        let is_disk_uptodate = status_str.contains("disk:UpToDate") ||
                              status_str.contains("disk:Consistent") ||
                              status_str.contains("disk:Inconsistent"); // Allow for initial sync

        // Check 3: At least one peer connection should be established (if peers exist)
        let has_connection = !status_str.contains("connection:StandAlone") ||
                            !status_str.lines().any(|line| line.contains("connection:"));

        // Check 4: Should not be in Diskless state
        let is_not_diskless = !status_str.contains("disk:Diskless");

        tracing::info!(
            "activate_profile: DRBD status check - Primary: {}, Disk OK: {}, Connection: {}, Not Diskless: {}",
            is_primary, is_disk_uptodate, has_connection, is_not_diskless
        );

        // Consider DRBD ready if basic conditions are met
        if is_primary && is_disk_uptodate && is_not_diskless {
            drbd_validated = true;
            tracing::info!("activate_profile: DRBD status validated successfully on attempt {}", validation_attempts);

            // If we're in StandAlone but there are no peers, that's OK for single-node setup
            if status_str.contains("connection:StandAlone") {
                tracing::info!("activate_profile: StandAlone mode detected (likely single-node setup)");
            }
        } else {
            tracing::warn!(
                "activate_profile: DRBD not ready on attempt {}, waiting 1 second...",
                validation_attempts
            );
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        }
    }

    if !drbd_validated {
        tracing::error!("activate_profile: DRBD status validation failed after {} attempts", MAX_VALIDATION_ATTEMPTS);
        state.send_progress(
            &operation_id,
            "activate_profile",
            Some(&profile.name),
            20,
            "DRBD status validation failed - timeout",
            true,
            Some(false),
        );
        state.send_notification(
            NotificationLevel::Error,
            "Activation Failed",
            &format!("DRBD resource '{}' failed to reach proper status within timeout", profile.resource_name),
        );
        return Err(AppError::Drbd(format!(
            "DRBD resource '{}' failed to reach proper status within {} seconds",
            profile.resource_name, MAX_VALIDATION_ATTEMPTS
        )));
    }

    // Additional check: Wait for synchronization if in progress
    let sync_cmd = format!("drbdadm status {}", profile.resource_name);
    let sync_status_output = run_shell_command(&sync_cmd, "Check sync status").await?;
    let sync_status_str = sync_status_output.stdout;

    // Look for sync progress indicators
    if sync_status_str.contains("sync:") {
        tracing::info!("activate_profile: DRBD synchronization in progress, waiting for completion...");
        state.send_progress(
            &operation_id,
            "activate_profile",
            Some(&profile.name),
            18,
            "Waiting for DRBD synchronization to complete...",
            false,
            None,
        );

        // Wait for sync to complete (with timeout)
        let mut sync_wait_attempts = 0;
        const MAX_SYNC_WAIT_ATTEMPTS: u32 = 300; // 5 minutes max

        while sync_wait_attempts < MAX_SYNC_WAIT_ATTEMPTS {
            sync_wait_attempts += 1;

            let sync_check_cmd = format!("drbdadm status {}", profile.resource_name);
            let sync_check_output = run_shell_command(&sync_check_cmd, "Check sync progress").await?;

            if !sync_check_output.stdout.contains("sync:") {
                tracing::info!("activate_profile: DRBD synchronization completed after {} attempts", sync_wait_attempts);
                break;
            }

            // Extract sync progress if available
            if let Some(sync_line) = sync_check_output.stdout.lines().find(|line| line.contains("sync:")) {
                tracing::info!("activate_profile: Sync progress: {}", sync_line.trim());

                // Parse sync percentage (DRBD 9.x format)
                if let Some(percent_str) = sync_line.split("sync'").nth(1).and_then(|s| s.split('%').next()) {
                    if let Ok(percent) = percent_str.trim().parse::<f32>() {
                        let progress_percent = 18 + (percent * 0.07).round() as u8; // Scale between 18-25%
                        state.send_progress(
                            &operation_id,
                            "activate_profile",
                            Some(&profile.name),
                            progress_percent,
                            &format!("Synchronizing... {:.1}%", percent),
                            false,
                            None,
                        );
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        }

        if sync_wait_attempts >= MAX_SYNC_WAIT_ATTEMPTS {
            tracing::warn!("activate_profile: Sync wait timeout, proceeding with activation");
            state.send_progress(
                &operation_id,
                "activate_profile",
                Some(&profile.name),
                25,
                "Sync timeout, proceeding with activation...",
                false,
                None,
            );
        }
    }

    let drbd_device = format!("/dev/drbd/by-res/{}/0", profile.resource_name);

    // Wait a moment for the device to be fully ready after promotion
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // Try multiple device paths in order of preference
    let device_paths = vec![
        format!(
            "/dev/drbd{}",
            profile.resource_name.parse::<u32>().unwrap_or(0)
        ), // Try direct device first
        drbd_device.clone(),
        format!(
            "/dev/drbd{}",
            status_check_out
                .stdout
                .lines()
                .find(|line| line.contains("device:"))
                .and_then(|line| line.split("device:").nth(1))
                .and_then(|dev| dev.trim().strip_prefix("/dev/drbd"))
                .unwrap_or("0")
        ),
    ];

    let mut working_device = None;
    let mut filesystem_type = None;

    for device_path in device_paths {
        tracing::info!("activate_profile: Trying device path: {}", device_path);

        // Check if device exists and is accessible
        let exists_cmd = format!("test -b {} && echo exists", device_path);
        if let Ok(exists_output) = run_shell_command(&exists_cmd, "Check device exists").await {
            if exists_output.stdout.contains("exists") {
                // Check for existing filesystem
                let check_fs_cmd =
                    format!("blkid -o value -s TYPE {} 2>/dev/null || true", device_path);
                if let Ok(fs_check) =
                    run_shell_command(&check_fs_cmd, "Check filesystem type").await
                {
                    if !fs_check.stdout.trim().is_empty() {
                        filesystem_type = Some(fs_check.stdout.trim().to_string());
                        tracing::info!(
                            "activate_profile: Found existing filesystem '{}' on {}",
                            fs_check.stdout.trim(),
                            device_path
                        );
                    }
                    working_device = Some(device_path);
                    break;
                }
            }
        }
    }

    let device_path = working_device
        .ok_or_else(|| AppError::Drbd("DRBD device not accessible after promotion".to_string()))?;

    // Only format if no filesystem was found and we didn't format during creation
    if filesystem_type.is_none() {
        // For a newly created HA profile, we need to determine which node should be the active one
        // Check if this node should be the primary by examining DRBD role and connections
        let current_drbd_status_cmd = format!("drbdadm status {}", profile.resource_name);
        let current_status_output = run_shell_command(&current_drbd_status_cmd, "Check current DRBD role").await?;

        let hostname_str = gethostname::gethostname().to_string_lossy().to_string();

        // Determine if this node should be the active one based on DRBD status
        let should_be_active = if current_status_output.stdout.contains("role:Primary") {
            // This node is already Primary, so it should handle activation
            tracing::info!("activate_profile: This node is Primary, will handle activation for profile '{}'", profile.name);
            true
        } else if current_status_output.stdout.contains("role:Secondary") {
            // This node is Secondary, let the Primary node handle activation
            tracing::info!("activate_profile: This node is Secondary, letting Primary node handle activation for profile '{}'", profile.name);
            false
        } else {
            // No clear role yet - use a deterministic approach based on hostname
            // This ensures only one node proceeds with activation
            let nodes_from_db = state.node_store.get_all()?;
            let mut sorted_hostnames = vec![hostname_str.clone()];

            // Add other nodes from database
            for node in nodes_from_db {
                if node.hostname != hostname_str && !sorted_hostnames.contains(&node.hostname) {
                    sorted_hostnames.push(node.hostname);
                }
            }

            sorted_hostnames.sort();

            // The first hostname alphabetically becomes the active node
            if sorted_hostnames.first() == Some(&hostname_str) {
                tracing::info!("activate_profile: This node ({}) is first alphabetically, will handle activation for profile '{}'", hostname_str, profile.name);
                true
            } else {
                tracing::info!("activate_profile: This node ({}) is not first alphabetically, letting {} handle activation for profile '{}'", hostname_str, sorted_hostnames.first().unwrap_or(&"unknown".to_string()), profile.name);
                false
            }
        };

        if !should_be_active {
            tracing::info!("activate_profile: This node is not the active node for profile '{}', letting drbd-reactor handle activation", profile.name);
            state.send_progress(
                &operation_id,
                "activate_profile",
                Some(&profile.name),
                25,
                "This node will be activated by drbd-reactor...",
                false,
                None,
            );

            // Don't format here - let the active node handle it
            // Return success since drbd-reactor will handle the actual activation
            return get_profile_status(State(state), Path(id)).await;
        }
        
        // Check if this is a fresh setup (no previous activation)
        let activation_marker = format!("/var/lib/drbd-ha/{}.activated", profile.resource_name);
        let marker_check = run_shell_command(
            &format!("test -f {} && echo exists", activation_marker),
            "Check activation marker"
        )
        .await;

        if marker_check
            .map(|o| o.stdout.contains("exists"))
            .unwrap_or(false)
        {
            tracing::warn!("activate_profile: No filesystem detected but device was previously activated. This may indicate a problem.");
            state.send_notification(
                NotificationLevel::Warning,
                "Filesystem Not Detected",
                &format!("No filesystem found on DRBD device for profile '{}', but device was previously activated. Please check device status.", profile.name),
            );
        } else {
            tracing::info!("activate_profile: No existing filesystem found, formatting device...");
            state.send_progress(
                &operation_id,
                "activate_profile",
                Some(&profile.name),
                25,
                &format!("Formatting {} filesystem...", profile.fs_type),
                false,
                None,
            );

            let mkfs_cmd = match profile.fs_type.as_str() {
                "xfs" => format!("mkfs.xfs -f {}", device_path),
                "ext4" => format!("mkfs.ext4 -F {}", device_path),
                _ => format!("mkfs.{} {}", profile.fs_type, device_path),
            };

            tracing::info!("activate_profile: Formatting with: {}", mkfs_cmd);
            let mkfs_output = run_shell_command(&mkfs_cmd, "Format filesystem").await?;

            if !mkfs_output.success() {
                return Err(AppError::Drbd(format!(
                    "Failed to format filesystem: {}",
                    mkfs_output.stderr
                )));
            }
            tracing::info!("activate_profile: Filesystem formatted successfully");

            // Create activation marker to avoid re-formatting
            let _ = run_shell_command(
                &format!("mkdir -p /var/lib/drbd-ha && touch {}", activation_marker),
                "Create activation marker",
            )
            .await;
        }
    }

    // Use the working device path for subsequent operations
    let drbd_device = device_path;

    state.send_progress(
        &operation_id,
        "activate_profile",
        Some(&profile.name),
        30,
        "Mounting DRBD device...",
        false,
        None,
    );

    let mount_cmd = format!(
        "mkdir -p {} && mount {} {}",
        profile.mount_point, drbd_device, profile.mount_point
    );
    tracing::info!("activate_profile: Running '{}'", mount_cmd);
    let mount_output = run_shell_command(
        &mount_cmd,
        &format!(
            "Mount DRBD device {} to {}",
            profile.resource_name, profile.mount_point
        ),
    )
    .await?;
    tracing::info!(
        "activate_profile: mount result: success={}, stderr='{}'",
        mount_output.success(),
        mount_output.stderr.trim()
    );

    let test_cmd = format!(
        "touch {}/.drbd-ha-test && rm {}/.drbd-ha-test",
        profile.mount_point, profile.mount_point
    );
    tracing::info!(
        "activate_profile: Testing write permission on {}",
        profile.mount_point
    );
    let test_output = run_shell_command(
        &test_cmd,
        &format!(
            "Test write permission on mount point {}",
            profile.mount_point
        ),
    )
    .await?;
    if !test_output.success() {
        tracing::warn!(
            "activate_profile: Mount point {} may have permission issues",
            profile.mount_point
        );
        state.send_notification(
            NotificationLevel::Warning,
            "Permission Warning",
            &format!("Mount point {} may have permission issues. Please check ownership (e.g., chown -R <user>:<group> {})", profile.mount_point, profile.mount_point)
        );
    } else {
        tracing::info!(
            "activate_profile: Mount point {} is writable",
            profile.mount_point
        );
    }

    state.send_progress(
        &operation_id,
        "activate_profile",
        Some(&profile.name),
        50,
        "Starting services...",
        false,
        None,
    );

    // Wait for drbd-reactor to generate the configuration and handle service management
    // This ensures the reactor is aware of the current state
    tracing::info!("activate_profile: Waiting for drbd-reactor to recognize the active state...");
    tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

    // Check drbd-reactor status to ensure it's managing this profile
    let reactor_check_cmd = format!("drbd-reactorctl status {} 2>/dev/null", profile.name);
    let reactor_check_output = run_shell_command(&reactor_check_cmd, "Check drbd-reactor status").await;

    if reactor_check_output.is_ok() && reactor_check_output.as_ref().unwrap().stdout.contains("active") {
        tracing::info!("activate_profile: drbd-reactor is managing profile '{}'", profile.name);

        // Let drbd-reactor handle the services automatically
        // We just need to ensure the services are enabled
        let systemd = SystemdController::new().await?;
        for service in &profile.promoter.services {
            tracing::info!("activate_profile: Enabling service '{}'", service);
            if let Err(e) = systemd.enable(service).await {
                tracing::warn!("activate_profile: Failed to enable service '{}': {}", service, e);
            } else {
                tracing::info!("activate_profile: Service '{}' enabled", service);
            }
        }

        // Give drbd-reactor a moment to start services
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        // Verify services are running
        for service in &profile.promoter.services {
            if let Ok(service_status) = systemd.status(service).await {
                if service_status.is_running() {
                    tracing::info!("activate_profile: Service '{}' is running", service);
                } else {
                    tracing::warn!("activate_profile: Service '{}' is not running ({}:{}), drbd-reactor may start it soon",
                        service, service_status.active_state, service_status.sub_state);
                }
            } else {
                tracing::warn!("activate_profile: Failed to check status of service '{}'", service);
            }
        }
    } else {
        tracing::info!("activate_profile: drbd-reactor not yet managing profile '{}', starting services manually", profile.name);

        // Start services manually if drbd-reactor is not yet managing the profile
        tracing::info!(
            "activate_profile: Starting {} services manually: {:?}",
            profile.promoter.services.len(),
            profile.promoter.services
        );
        let systemd = SystemdController::new().await?;
        for service in &profile.promoter.services {
            tracing::info!("activate_profile: Starting service '{}'", service);
            systemd.start(service).await?;
            tracing::info!("activate_profile: Service '{}' started", service);
        }
    }

    if let Some(vip) = &profile.vip {
        tracing::info!(
            "activate_profile: VIP {}/{} will be configured by drbd-reactor",
            vip.address,
            vip.netmask
        );
        state.send_progress(
            &operation_id,
            "activate_profile",
            Some(&profile.name),
            80,
            "VIP will be configured by drbd-reactor...",
            false,
            None,
        );
    }

    state.send_progress(
        &operation_id,
        "activate_profile",
        Some(&profile.name),
        90,
        "Reloading drbd-reactor on all nodes...",
        false,
        None,
    );
    
    // Only reload drbd-reactor after all operations are complete
    // This ensures the reactor doesn't interfere with the activation process
    let _ = reload_reactor(
        State(state.clone()),
        Json(ReactorReloadRequest {
            action: "reload".to_string(),
        }),
    )
    .await;

    state.send_progress(
        &operation_id,
        "activate_profile",
        Some(&profile.name),
        100,
        "Profile activated successfully",
        true,
        Some(true),
    );
    state.send_notification(
        NotificationLevel::Success,
        "Profile Activated",
        &format!("HA profile '{}' is now active", profile.name),
    );

    get_profile_status(State(state), Path(id)).await
}

/// POST /api/v1/ha/profiles/:id/deactivate
#[utoipa::path(
    post,
    path = "/api/v1/ha/profiles/{id}/deactivate",
    tag = "ha",
    params(
        ("id" = String, Path, description = "Profile ID or name")
    ),
    responses(
        (status = 200, description = "Profile deactivated", body = HaProfileDetailResponse),
        (status = 404, description = "Profile not found")
    )
)]
pub async fn deactivate_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<HaProfileDetailResponse>> {
    tracing::info!(
        "deactivate_profile: Starting deactivation for profile id={}",
        id
    );

    // Load profile from toml file instead of database
    let config_path = crate::core::ReactorConfigPaths::promoter_path(&id);
    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|_| AppError::NotFound(format!("HA profile {} not found", id)))?;
    let profile = create_profile_from_toml(&id, &content)
        .ok_or_else(|| AppError::NotFound(format!("HA profile {} not found", id)))?;

    tracing::info!(
        "deactivate_profile: Found profile '{}', resource='{}', mount='{}'",
        profile.name,
        profile.resource_name,
        profile.mount_point
    );

    let operation_id = uuid::Uuid::new_v4().to_string();

    if let Some(vip) = &profile.vip {
        tracing::info!(
            "deactivate_profile: VIP {}/{} was managed by drbd-reactor",
            vip.address,
            vip.netmask
        );
        state.send_progress(
            &operation_id,
            "deactivate_profile",
            Some(&profile.name),
            0,
            "VIP was managed by drbd-reactor...",
            false,
            None,
        );
    }

    state.send_progress(
        &operation_id,
        "deactivate_profile",
        Some(&profile.name),
        20,
        "Stopping services...",
        false,
        None,
    );

    tracing::info!(
        "deactivate_profile: Stopping {} services in reverse order: {:?}",
        profile.promoter.services.len(),
        profile.promoter.services.iter().rev().collect::<Vec<_>>()
    );
    let systemd = SystemdController::new().await?;
    for service in profile.promoter.services.iter().rev() {
        tracing::info!("deactivate_profile: Stopping service '{}'", service);
        if let Err(e) = systemd.stop(service).await {
            tracing::warn!(
                "deactivate_profile: Failed to stop service {}: {}",
                service,
                e
            );
        } else {
            tracing::info!("deactivate_profile: Service '{}' stopped", service);
        }
    }

    tracing::info!("deactivate_profile: Waiting 500ms for services to release file handles");
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    state.send_progress(
        &operation_id,
        "deactivate_profile",
        Some(&profile.name),
        40,
        "Releasing mount point...",
        false,
        None,
    );
    let my_pid = std::process::id();
    tracing::info!(
        "deactivate_profile: Killing processes using {} (excluding pid={})",
        profile.mount_point,
        my_pid
    );
    let kill_cmd = format!(
        "for pid in $(lsof -t {} 2>/dev/null | grep -v {}); do kill -9 $pid 2>/dev/null || true; done",
        profile.mount_point, my_pid
    );
    let kill_output = run_shell_command(
        &kill_cmd,
        &format!("Kill processes using mount point {}", profile.mount_point),
    )
    .await;
    tracing::info!("deactivate_profile: Kill result: {:?}", kill_output);

    tracing::info!("deactivate_profile: Waiting 500ms for processes to terminate");
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    state.send_progress(
        &operation_id,
        "deactivate_profile",
        Some(&profile.name),
        50,
        "Unmounting DRBD device...",
        false,
        None,
    );

    let mut unmounted = false;
    for attempt in 1..=3 {
        let umount_cmd = format!("umount {}", profile.mount_point);
        tracing::info!(
            "deactivate_profile: Unmount attempt {}: '{}'",
            attempt,
            umount_cmd
        );
        let output = run_shell_command(
            &umount_cmd,
            &format!("Unmount {} (attempt {})", profile.mount_point, attempt),
        )
        .await?;
        tracing::info!(
            "deactivate_profile: Unmount result: success={}, stderr='{}'",
            output.success(),
            output.stderr.trim()
        );
        if output.success() {
            unmounted = true;
            tracing::info!(
                "deactivate_profile: Unmount succeeded on attempt {}",
                attempt
            );
            break;
        }
        tracing::warn!(
            "deactivate_profile: Unmount attempt {} failed: {}",
            attempt,
            output.stderr
        );
        if attempt < 3 {
            tracing::info!(
                "deactivate_profile: Retrying kill of processes using {}",
                profile.mount_point
            );
            let retry_kill_cmd = format!(
                "for pid in $(lsof -t {} 2>/dev/null | grep -v {}); do kill -9 $pid 2>/dev/null || true; done",
                profile.mount_point, my_pid
            );
            let _ = run_shell_command(
                &retry_kill_cmd,
                &format!(
                    "Retry killing processes using mount point {}",
                    profile.mount_point
                ),
            )
            .await;
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }

    if !unmounted {
        tracing::info!("deactivate_profile: Regular unmount failed, trying lazy unmount");
        let lazy_umount_cmd = format!("umount -l {}", profile.mount_point);
        tracing::info!("deactivate_profile: Running '{}'", lazy_umount_cmd);
        let output = run_shell_command(
            &lazy_umount_cmd,
            &format!("Lazy unmount {}", profile.mount_point),
        )
        .await?;
        tracing::info!(
            "deactivate_profile: Lazy unmount result: success={}, stderr='{}'",
            output.success(),
            output.stderr.trim()
        );
        if !output.success() {
            tracing::error!("deactivate_profile: Lazy unmount failed: {}", output.stderr);
            state.send_progress(
                &operation_id,
                "deactivate_profile",
                Some(&profile.name),
                60,
                &format!("Failed to unmount: {}", output.stderr),
                true,
                Some(false),
            );
            state.send_notification(
                NotificationLevel::Error,
                "Deactivation Failed",
                &format!(
                    "Failed to unmount '{}': {}",
                    profile.mount_point, output.stderr
                ),
            );
            return Err(AppError::Drbd(format!(
                "Failed to unmount {}: {}",
                profile.mount_point, output.stderr
            )));
        }
        tracing::info!("deactivate_profile: Waiting 1000ms for lazy unmount to complete");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    state.send_progress(
        &operation_id,
        "deactivate_profile",
        Some(&profile.name),
        70,
        "Demoting DRBD resource...",
        false,
        None,
    );

    let demote_cmd = format!("drbdadm secondary {}", profile.resource_name);
    tracing::info!("deactivate_profile: Running '{}'", demote_cmd);
    let output = run_shell_command(
        &demote_cmd,
        &format!("Demote DRBD resource {}", profile.resource_name),
    )
    .await?;
    tracing::info!(
        "deactivate_profile: Demote result: success={}, stderr='{}'",
        output.success(),
        output.stderr.trim()
    );
    if !output.success() {
        tracing::error!("deactivate_profile: Demote failed: {}", output.stderr);
        state.send_progress(
            &operation_id,
            "deactivate_profile",
            Some(&profile.name),
            80,
            &format!("Failed to demote: {}", output.stderr),
            true,
            Some(false),
        );
        state.send_notification(
            NotificationLevel::Error,
            "Deactivation Failed",
            &format!("Failed to demote '{}': {}", profile.name, output.stderr),
        );
        return Err(AppError::Drbd(format!(
            "Failed to demote resource: {}",
            output.stderr
        )));
    }

    state.send_progress(
        &operation_id,
        "deactivate_profile",
        Some(&profile.name),
        80,
        "Bringing down DRBD resource...",
        false,
        None,
    );

    // Bring down DRBD resource after demotion
    let down_cmd = format!("drbdadm down {}", profile.resource_name);
    tracing::info!("deactivate_profile: Running '{}'", down_cmd);
    let down_output = run_shell_command(
        &down_cmd,
        &format!("Bring down DRBD resource {}", profile.resource_name),
    )
    .await?;
    tracing::info!(
        "deactivate_profile: Down result: success={}, stderr='{}'",
        down_output.success(),
        down_output.stderr.trim()
    );

    state.send_progress(
        &operation_id,
        "deactivate_profile",
        Some(&profile.name),
        85,
        "Removing drbd-reactor configuration...",
        false,
        None,
    );

    // Remove drbd-reactor configuration file
    let config_path = state.reactor_config_path(&profile.name);
    tracing::info!("deactivate_profile: Removing config file '{}'", config_path);
    let _ = run_shell_command(
        &format!("rm -f '{}'", config_path),
        &format!("Remove drbd-reactor config for {}", profile.name),
    )
    .await;

    state.send_progress(
        &operation_id,
        "deactivate_profile",
        Some(&profile.name),
        90,
        "Reloading drbd-reactor...",
        false,
        None,
    );

    // Reload drbd-reactor to pick up configuration changes
    tracing::info!("deactivate_profile: Reloading drbd-reactor...");
    let _ = reload_reactor(
        State(state.clone()),
        Json(ReactorReloadRequest {
            action: "reload".to_string(),
        }),
    )
    .await;

    tracing::info!(
        "deactivate_profile: Profile '{}' deactivated successfully",
        profile.name
    );
    state.send_progress(
        &operation_id,
        "deactivate_profile",
        Some(&profile.name),
        100,
        "Profile deactivated successfully",
        true,
        Some(true),
    );
    state.send_notification(
        NotificationLevel::Info,
        "Profile Deactivated",
        &format!("HA profile '{}' is now standby", profile.name),
    );

    get_profile_status(State(state), Path(id)).await
}

use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::core::{run_shell_command, systemd_ctrl::SystemdController, NfsGenerator};
use crate::error::{AppError, AppResult};
use crate::models::HaType;
use crate::state::{AppState, NotificationLevel};

use super::crud::get_profile_status;
use super::reactor::reload_reactor;
use super::types::{HaProfileDetailResponse, ReactorReloadRequest};

/// POST /api/v1/ha/profiles/:id/activate
pub async fn activate_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<HaProfileDetailResponse>> {
    tracing::info!(
        "activate_profile: Starting activation for profile id={}",
        id
    );

    let profile = state
        .db
        .get_ha_profile(&id)?
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

    let drbd_device = format!("/dev/drbd/by-res/{}/0", profile.resource_name);
    let check_fs_cmd = format!("blkid -o value -s TYPE {}", drbd_device);
    tracing::info!("activate_profile: Checking filesystem on {}", drbd_device);
    let fs_check = run_shell_command(&check_fs_cmd, "Check filesystem type").await?;

    if fs_check.stdout.trim().is_empty() {
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
            "xfs" => format!("mkfs.xfs -f {}", drbd_device),
            "ext4" => format!("mkfs.ext4 -F {}", drbd_device),
            _ => format!("mkfs.{} {}", profile.fs_type, drbd_device),
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
    } else {
        tracing::info!(
            "activate_profile: Existing filesystem found: {}",
            fs_check.stdout.trim()
        );
    }

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

    if profile.ha_type == HaType::Nfs {
        tracing::info!("activate_profile: Setting up NFS state directory");
        state.send_progress(
            &operation_id,
            "activate_profile",
            Some(&profile.name),
            40,
            "Setting up NFS state directory...",
            false,
            None,
        );

        if let Err(e) = NfsGenerator::setup_nfs_state(&profile.mount_point).await {
            tracing::error!("Failed to setup NFS state: {}", e);
            return Err(e);
        }
        tracing::info!("activate_profile: NFS state directory setup complete");

        tracing::info!("activate_profile: Refreshing NFS exports");
        crate::core::run_shell_command("exportfs -ra", "Refresh NFS exports").await?;
        tracing::info!("activate_profile: NFS exports refreshed");
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

    tracing::info!(
        "activate_profile: Starting {} services: {:?}",
        profile.promoter.services.len(),
        profile.promoter.services
    );
    let systemd = SystemdController::new().await?;
    for service in &profile.promoter.services {
        tracing::info!("activate_profile: Starting service '{}'", service);
        systemd.start(service).await?;
        tracing::info!("activate_profile: Service '{}' started", service);
    }

    if let Some(vip) = &profile.vip {
        tracing::info!(
            "activate_profile: Configuring VIP {}/{} on {}",
            vip.address,
            vip.netmask,
            vip.interface
        );
        state.send_progress(
            &operation_id,
            "activate_profile",
            Some(&profile.name),
            80,
            "Configuring VIP...",
            false,
            None,
        );
        let vip_cmd = format!(
            "ip addr add {}/{} dev {}",
            vip.address, vip.netmask, vip.interface
        );
        let _ = run_shell_command(
            &vip_cmd,
            &format!(
                "Configure VIP {}/{} on {}",
                vip.address, vip.netmask, vip.interface
            ),
        )
        .await;
    }

    state.send_progress(
        &operation_id,
        "activate_profile",
        Some(&profile.name),
        90,
        "Restarting drbd-reactor on all nodes...",
        false,
        None,
    );
    let _ = reload_reactor(
        State(state.clone()),
        Json(ReactorReloadRequest {
            action: "restart".to_string(),
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
pub async fn deactivate_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<HaProfileDetailResponse>> {
    tracing::info!(
        "deactivate_profile: Starting deactivation for profile id={}",
        id
    );

    let profile = state
        .db
        .get_ha_profile(&id)?
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
            "deactivate_profile: Removing VIP {}/{} from {}",
            vip.address,
            vip.netmask,
            vip.interface
        );
        state.send_progress(
            &operation_id,
            "deactivate_profile",
            Some(&profile.name),
            0,
            "Removing VIP...",
            false,
            None,
        );
        let vip_cmd = format!(
            "ip addr del {}/{} dev {} 2>/dev/null || true",
            vip.address, vip.netmask, vip.interface
        );
        let vip_output = run_shell_command(
            &vip_cmd,
            &format!(
                "Remove VIP {}/{} from {}",
                vip.address, vip.netmask, vip.interface
            ),
        )
        .await;
        tracing::info!("deactivate_profile: VIP remove result: {:?}", vip_output);
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

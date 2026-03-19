//! Data Migration Module
//!
//! This module handles migrating existing service data from local directories
//! to DRBD-backed storage. It ensures safe data transfer with proper service
//! shutdown, DRBD promotion, filesystem formatting, and data synchronization.

pub mod error;

use crate::error::{MigrationError, MigrationResult};
use serde::{Deserialize, Serialize};
use shell_cmd::run_shell_command;
use systemd_utils::SystemdController;

/// Data migration handler
pub struct DataMigration;

/// Progress callback type for migration status updates
pub type ProgressCallback = Box<dyn Fn(MigrationProgress) + Send + Sync>;

impl DataMigration {
    /// Migrate data from source directory to DRBD device
    pub async fn migrate(
        config: MigrationConfig,
        progress_cb: Option<ProgressCallback>,
    ) -> MigrationResult<MigrationResultStats> {
        let report_progress = |stage: MigrationStage, message: &str| {
            if let Some(ref cb) = progress_cb {
                cb(MigrationProgress {
                    stage: stage.clone(),
                    message: message.to_string(),
                    percent: stage.percent(),
                });
            }
            tracing::info!("[Migration] {}: {}", stage.name(), message);
        };

        // Validate source path exists
        if tokio::fs::metadata(&config.source_path).await.is_err() {
            return Err(MigrationError::Validation(format!(
                "Source path does not exist: {}",
                config.source_path
            )));
        }

        // Check if source has data
        let source_size = Self::get_directory_size(&config.source_path).await?;
        report_progress(
            MigrationStage::Preparing,
            &format!("Source directory size: {} bytes", source_size),
        );

        // Step 1: Stop services if specified
        let mut stopped_services = Vec::new();
        if !config.services_to_stop.is_empty() {
            report_progress(
                MigrationStage::StoppingServices,
                &format!("Stopping {} service(s)", config.services_to_stop.len()),
            );

            for service in &config.services_to_stop {
                if Self::stop_service(service).await? {
                    stopped_services.push(service.clone());
                }
            }
        }

        // Step 2: Promote DRBD to Primary
        report_progress(
            MigrationStage::PromotingDrbd,
            &format!("Promoting DRBD resource: {}", config.resource_name),
        );
        Self::promote_drbd(&config.resource_name).await?;

        // Step 3: Get DRBD device path
        let device_path = Self::get_drbd_device(&config.resource_name).await?;
        report_progress(
            MigrationStage::PromotingDrbd,
            &format!("DRBD device: {}", device_path),
        );

        // Step 4: Format filesystem if requested
        if config.format_device {
            report_progress(
                MigrationStage::FormattingDevice,
                &format!("Formatting device with {}", config.fs_type),
            );
            Self::format_device(&device_path, &config.fs_type).await?;
        }

        // Step 5: Create temporary mount point and mount
        let temp_mount = format!("/tmp/drbd-migrate-{}", config.resource_name);
        tokio::fs::create_dir_all(&temp_mount).await?;

        report_progress(
            MigrationStage::MountingDevice,
            &format!("Mounting to {}", temp_mount),
        );
        Self::mount_device(&device_path, &temp_mount, &config.fs_type).await?;

        // Step 6: Sync data with rsync
        report_progress(
            MigrationStage::SyncingData,
            "Starting data synchronization with rsync",
        );

        let rsync_result = Self::rsync_data(
            &config.source_path,
            &temp_mount,
            config.preserve_permissions,
        )
        .await;

        // Ensure we unmount even if rsync fails
        let unmount_result = Self::unmount_device(&temp_mount).await;
        let _ = tokio::fs::remove_dir(&temp_mount).await;

        // Handle rsync result after cleanup
        let bytes_transferred = rsync_result?;

        // Handle unmount result
        unmount_result?;

        // Step 7: Demote DRBD back to Secondary (if not requested to keep Primary)
        if !config.keep_primary {
            report_progress(MigrationStage::Finalizing, "Demoting DRBD resource");
            Self::demote_drbd(&config.resource_name).await?;
        } else {
            report_progress(
                MigrationStage::Finalizing,
                "Keeping DRBD resource as Primary",
            );
        }

        // Step 8: Restart services if they were stopped
        if !stopped_services.is_empty() {
            report_progress(
                MigrationStage::Finalizing,
                &format!("Restarting {} service(s)", stopped_services.len()),
            );

            for service in &stopped_services {
                Self::start_service(service).await?;
            }
        }

        report_progress(
            MigrationStage::Completed,
            "Migration completed successfully",
        );

        Ok(MigrationResultStats {
            source_path: config.source_path,
            resource_name: config.resource_name,
            bytes_transferred,
            services_restarted: stopped_services,
        })
    }

    /// Stop a systemd service
    async fn stop_service(service: &str) -> MigrationResult<bool> {
        let sys = SystemdController::new().await?;
        // Check if service is active first
        let status = sys.status(service).await?;

        if status.active_state != "active" {
            tracing::debug!("Service {} is not active, skipping stop", service);
            return Ok(false);
        }

        sys.stop(service).await?;

        tracing::info!("Stopped service: {}", service);
        Ok(true)
    }

    /// Start a systemd service
    async fn start_service(service: &str) -> MigrationResult<()> {
        let sys = SystemdController::new().await?;
        sys.start(service).await?;

        tracing::info!("Started service: {}", service);
        Ok(())
    }

    /// Promote DRBD resource to Primary
    async fn promote_drbd(resource_name: &str) -> MigrationResult<()> {
        let cmd = format!("drbdadm primary {}", resource_name);
        let output = run_shell_command(&cmd, &format!("Promote DRBD resource {}", resource_name))
            .await
            .map_err(|e| MigrationError::Command(e.to_string()))?;

        if !output.success() {
            return Err(MigrationError::Drbd(format!(
                "Failed to promote DRBD resource {}: {}",
                resource_name, output.stderr
            )));
        }

        // Wait for promotion to complete
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        Ok(())
    }

    /// Demote DRBD resource to Secondary
    async fn demote_drbd(resource_name: &str) -> MigrationResult<()> {
        let cmd = format!("drbdadm secondary {}", resource_name);
        let output = run_shell_command(&cmd, &format!("Demote DRBD resource {}", resource_name))
            .await
            .map_err(|e| MigrationError::Command(e.to_string()))?;

        if !output.success() {
            return Err(MigrationError::Drbd(format!(
                "Failed to demote DRBD resource {}: {}",
                resource_name, output.stderr
            )));
        }

        Ok(())
    }

    /// Get DRBD device path for a resource
    async fn get_drbd_device(resource_name: &str) -> MigrationResult<String> {
        // Try by-res symlink first
        let by_res_path = format!("/dev/drbd/by-res/{}/0", resource_name);
        if tokio::fs::metadata(&by_res_path).await.is_ok() {
            return Ok(by_res_path);
        }

        // Query drbdadm
        let cmd = format!("drbdadm sh-dev {} 2>/dev/null", resource_name);
        let output =
            run_shell_command(&cmd, &format!("Get DRBD device path for {}", resource_name))
                .await
                .map_err(|e| MigrationError::Command(e.to_string()))?;

        if output.success() && !output.stdout.trim().is_empty() {
            return Ok(output.stdout.trim().to_string());
        }

        Err(MigrationError::Drbd(format!(
            "Could not determine device path for DRBD resource: {}",
            resource_name
        )))
    }

    /// Format a device with the specified filesystem
    async fn format_device(device: &str, fs_type: &str) -> MigrationResult<()> {
        let cmd = match fs_type {
            "xfs" => format!("mkfs.xfs -f {}", device),
            "ext4" => format!("mkfs.ext4 -F {}", device),
            "btrfs" => format!("mkfs.btrfs -f {}", device),
            other => {
                return Err(MigrationError::Validation(format!(
                    "Unsupported filesystem type: {}",
                    other
                )))
            }
        };

        let output = run_shell_command(&cmd, &format!("Format device {} with {}", device, fs_type))
            .await
            .map_err(|e| MigrationError::Command(e.to_string()))?;

        if !output.success() {
            return Err(MigrationError::Command(format!(
                "Failed to format device {}: {}",
                device, output.stderr
            )));
        }

        tracing::info!("Formatted {} with {}", device, fs_type);
        Ok(())
    }

    /// Mount a device to a directory
    async fn mount_device(device: &str, mount_point: &str, fs_type: &str) -> MigrationResult<()> {
        let cmd = format!("mount -t {} {} {}", fs_type, device, mount_point);
        let output =
            run_shell_command(&cmd, &format!("Mount device {} to {}", device, mount_point))
                .await
                .map_err(|e| MigrationError::Command(e.to_string()))?;

        if !output.success() {
            return Err(MigrationError::Command(format!(
                "Failed to mount {} to {}: {}",
                device, mount_point, output.stderr
            )));
        }

        tracing::info!("Mounted {} to {}", device, mount_point);
        Ok(())
    }

    /// Unmount a device
    async fn unmount_device(mount_point: &str) -> MigrationResult<()> {
        // Sync before unmounting
        let _ = run_shell_command("sync", "Syncing filesystem before unmount").await;

        let cmd = format!("umount {}", mount_point);
        let output = run_shell_command(&cmd, &format!("Unmount {}", mount_point))
            .await
            .map_err(|e| MigrationError::Command(e.to_string()))?;

        if !output.success() {
            return Err(MigrationError::Command(format!(
                "Failed to unmount {}: {}",
                mount_point, output.stderr
            )));
        }

        tracing::info!("Unmounted {}", mount_point);
        Ok(())
    }

    /// Sync data using rsync
    async fn rsync_data(
        source: &str,
        destination: &str,
        preserve_permissions: bool,
    ) -> MigrationResult<u64> {
        // Build rsync command
        let mut flags = String::from("-avH");
        if preserve_permissions {
            flags.push_str("AX");
        }

        // Ensure source path ends with / to copy contents, not directory itself
        let source_path = if source.ends_with('/') {
            source.to_string()
        } else {
            format!("{}/", source)
        };

        let cmd = format!(
            "rsync {} --delete --stats '{}' '{}'",
            flags, source_path, destination
        );

        tracing::debug!("Running rsync: {}", cmd);
        let output = run_shell_command(
            &cmd,
            &format!("Rsync data from {} to {}", source_path, destination),
        )
        .await
        .map_err(|e| MigrationError::Command(e.to_string()))?;

        if !output.success() {
            return Err(MigrationError::Command(format!(
                "rsync failed: {}",
                output.stderr
            )));
        }

        // Parse bytes transferred from rsync stats output
        let bytes = Self::parse_rsync_bytes(&output.stdout);
        tracing::info!("rsync completed, {} bytes transferred", bytes);

        Ok(bytes)
    }

    /// Parse bytes transferred from rsync output
    fn parse_rsync_bytes(output: &str) -> u64 {
        for line in output.lines() {
            if line.contains("Total transferred file size:") || line.contains("total size is") {
                let numbers: String = line.chars().filter(|c| c.is_ascii_digit()).collect();
                if let Ok(bytes) = numbers.parse::<u64>() {
                    return bytes;
                }
            }
        }
        0
    }

    /// Get the size of a directory in bytes
    async fn get_directory_size(path: &str) -> MigrationResult<u64> {
        let cmd = format!("du -sb '{}' 2>/dev/null | cut -f1", path);
        let output = run_shell_command(&cmd, &format!("Get size of directory {}", path))
            .await
            .map_err(|e| MigrationError::Command(e.to_string()))?;

        if output.success() {
            if let Ok(size) = output.stdout.trim().parse::<u64>() {
                return Ok(size);
            }
        }

        Ok(0)
    }

    /// Verify data integrity after migration
    pub async fn verify_migration(
        source: &str,
        resource_name: &str,
        _mount_point: &str,
        fs_type: &str,
    ) -> MigrationResult<VerificationResult> {
        // Promote and mount temporarily
        Self::promote_drbd(resource_name).await?;

        let device = Self::get_drbd_device(resource_name).await?;
        let temp_mount = format!("/tmp/drbd-verify-{}", resource_name);
        tokio::fs::create_dir_all(&temp_mount).await?;

        Self::mount_device(&device, &temp_mount, fs_type).await?;

        // Run diff to compare
        let cmd = format!("diff -rq '{}' '{}' 2>&1 | head -20", source, temp_mount);
        let output = run_shell_command(
            &cmd,
            &format!(
                "Verify migration differences between {} and {}",
                source, temp_mount
            ),
        )
        .await
        .map_err(|e| MigrationError::Command(e.to_string()))?;

        let differences = output.stdout.lines().count();
        let is_identical = differences == 0;

        // Cleanup
        let _ = Self::unmount_device(&temp_mount).await;
        let _ = tokio::fs::remove_dir(&temp_mount).await;
        let _ = Self::demote_drbd(resource_name).await;

        Ok(VerificationResult {
            is_identical,
            differences_found: differences,
            sample_differences: if is_identical {
                Vec::new()
            } else {
                output.stdout.lines().take(10).map(String::from).collect()
            },
        })
    }
}

/// Configuration for data migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    pub resource_name: String,
    pub source_path: String,
    pub mount_point: String,
    pub fs_type: String,
    pub format_device: bool,
    pub services_to_stop: Vec<String>,
    pub preserve_permissions: bool,
    pub keep_primary: bool,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            resource_name: String::new(),
            source_path: String::new(),
            mount_point: String::new(),
            fs_type: "xfs".to_string(),
            format_device: true,
            services_to_stop: Vec::new(),
            preserve_permissions: true,
            keep_primary: false,
        }
    }
}

/// Result of a data migration operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationResultStats {
    pub source_path: String,
    pub resource_name: String,
    pub bytes_transferred: u64,
    pub services_restarted: Vec<String>,
}

/// Migration progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationProgress {
    pub stage: MigrationStage,
    pub message: String,
    pub percent: u8,
}

/// Stages of the migration process
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MigrationStage {
    Preparing,
    StoppingServices,
    PromotingDrbd,
    FormattingDevice,
    MountingDevice,
    SyncingData,
    Finalizing,
    Completed,
}

impl MigrationStage {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Preparing => "Preparing",
            Self::StoppingServices => "Stopping Services",
            Self::PromotingDrbd => "Promoting DRBD",
            Self::FormattingDevice => "Formatting Device",
            Self::MountingDevice => "Mounting Device",
            Self::SyncingData => "Syncing Data",
            Self::Finalizing => "Finalizing",
            Self::Completed => "Completed",
        }
    }

    pub fn percent(&self) -> u8 {
        match self {
            Self::Preparing => 5,
            Self::StoppingServices => 10,
            Self::PromotingDrbd => 20,
            Self::FormattingDevice => 30,
            Self::MountingDevice => 40,
            Self::SyncingData => 70,
            Self::Finalizing => 90,
            Self::Completed => 100,
        }
    }
}

/// Result of migration verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub is_identical: bool,
    pub differences_found: usize,
    pub sample_differences: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rsync_bytes() {
        let output = r#"
Number of files: 1,234
Total file size: 5,678,901 bytes
Total transferred file size: 1234567 bytes
"#;
        assert_eq!(DataMigration::parse_rsync_bytes(output), 1234567);
    }
}

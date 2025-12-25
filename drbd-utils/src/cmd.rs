use crate::error::{DrbdError, DrbdResult};
use crate::models::{DrbdStatus, ResourceStatus};
use crate::validator;

/// DRBD command builder
pub struct DrbdCmd;

impl DrbdCmd {
    /// Get status command
    pub fn status_cmd() -> String {
        "drbdsetup status --json".to_string()
    }

    /// Get resource status command
    pub fn resource_status_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdsetup status {} --json", resource))
    }

    /// Create resource command (drbdadm create-md)
    pub fn create_md_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        // Use --force to skip confirmation prompts
        Ok(format!("drbdadm create-md --force {}", resource))
    }

    /// Adjust resource command (applies configuration)
    pub fn adjust_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm adjust {}", resource))
    }

    /// Up resource command
    pub fn up_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm up {}", resource))
    }

    /// Down resource command
    pub fn down_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm down {}", resource))
    }

    /// Primary command
    pub fn primary_cmd(resource: &str, force: bool) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        if force {
            Ok(format!("drbdadm primary {} --force", resource))
        } else {
            Ok(format!("drbdadm primary {}", resource))
        }
    }

    /// Secondary command
    pub fn secondary_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm secondary {}", resource))
    }

    /// Connect command
    pub fn connect_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm connect {}", resource))
    }

    /// Disconnect command
    pub fn disconnect_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm disconnect {}", resource))
    }

    /// Invalidate command (start resync from peer)
    pub fn invalidate_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm invalidate {}", resource))
    }

    /// Verify command
    pub fn verify_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm verify {}", resource))
    }

    /// Connect with discard-my-data (for split brain recovery as victim)
    pub fn connect_discard_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm connect --discard-my-data {}", resource))
    }

    /// Pause sync command
    pub fn pause_sync_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm pause-sync {}", resource))
    }

    /// Resume sync command
    pub fn resume_sync_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm resume-sync {}", resource))
    }

    /// Resize command
    pub fn resize_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm resize {}", resource))
    }

    /// Dump configuration
    pub fn dump_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm dump {}", resource))
    }

    /// Check configuration validity
    pub fn check_config_cmd() -> String {
        "drbdadm sh-nop all".to_string()
    }

    /// Get DRBD kernel module version
    pub fn version_cmd() -> String {
        "cat /proc/drbd 2>/dev/null || drbdadm --version".to_string()
    }

    /// Load DRBD kernel module
    pub fn load_module_cmd() -> String {
        "modprobe drbd".to_string()
    }

    /// Get device path for a resource (drbdadm sh-dev)
    pub fn sh_dev_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm sh-dev {}", resource))
    }

    /// Get status for all resources (drbdadm status)
    pub fn status_all_cmd() -> String {
        "drbdadm status".to_string()
    }

    /// Get status for a specific resource (drbdadm status)
    pub fn adm_status_cmd(resource: &str) -> DrbdResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm status {}", resource))
    }

    /// Create filesystem on DRBD device
    pub fn mkfs_cmd(device: &str, fstype: &str) -> DrbdResult<String> {
        validator::validate_block_device(device)?;
        // Validate fstype
        let allowed_fstypes = ["ext4", "xfs", "btrfs"];
        if !allowed_fstypes.contains(&fstype) {
            return Err(DrbdError::Validation(format!(
                "Invalid filesystem type '{}'. Allowed: {:?}",
                fstype, allowed_fstypes
            )));
        }
        Ok(format!("mkfs.{} {}", fstype, device))
    }

    /// Mount DRBD device
    pub fn mount_cmd(device: &str, mount_point: &str) -> DrbdResult<String> {
        validator::validate_block_device(device)?;
        validator::validate_mount_point(mount_point)?;
        Ok(format!("mount {} {}", device, mount_point))
    }

    /// Unmount DRBD device
    pub fn umount_cmd(mount_point: &str) -> DrbdResult<String> {
        validator::validate_mount_point(mount_point)?;
        Ok(format!("umount {}", mount_point))
    }

    /// Create mount point directory
    pub fn mkdir_cmd(mount_point: &str) -> DrbdResult<String> {
        validator::validate_mount_point(mount_point)?;
        Ok(format!("mkdir -p {}", mount_point))
    }
}

/// Parse DRBD status JSON output
pub fn parse_drbd_status(json_output: &str) -> DrbdResult<Vec<ResourceStatus>> {
    // Handle empty output
    if json_output.trim().is_empty() {
        return Ok(Vec::new());
    }

    // Try parsing as array first (drbdsetup status --json returns array)
    if let Ok(resources) = serde_json::from_str::<Vec<ResourceStatus>>(json_output) {
        return Ok(resources);
    }

    // Try parsing as object with resources field
    if let Ok(status) = serde_json::from_str::<DrbdStatus>(json_output) {
        return Ok(status.resources);
    }

    Err(DrbdError::JsonParse(format!(
        "Failed to parse DRBD status JSON: {}",
        json_output
    )))
}

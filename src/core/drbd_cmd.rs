//! DRBD command wrapper
//!
//! Wraps drbdadm and drbdsetup commands for resource management.

use crate::core::validator;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};

/// DRBD status from drbdsetup status --json
#[derive(Debug, Deserialize)]
pub struct DrbdStatus {
    #[serde(default)]
    pub resources: Vec<ResourceStatus>,
}

/// Individual resource status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStatus {
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub devices: Vec<DeviceStatus>,
    #[serde(default)]
    pub connections: Vec<ConnectionStatus>,
}

/// Device status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub volume: u32,
    #[serde(rename = "disk-state")]
    pub disk_state: String,
    pub minor: u32,
    #[serde(default)]
    pub size: Option<u64>,
}

/// Connection status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatus {
    #[serde(rename = "peer-node-id")]
    pub peer_node_id: u32,
    pub name: String,
    #[serde(rename = "connection-state")]
    pub connection_state: String,
    #[serde(default)]
    pub peer_devices: Vec<PeerDeviceStatus>,
}

/// Peer device status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerDeviceStatus {
    pub volume: u32,
    #[serde(rename = "replication-state")]
    pub replication_state: String,
    #[serde(rename = "peer-disk-state")]
    pub peer_disk_state: String,
    #[serde(rename = "percent-in-sync", default)]
    pub percent_in_sync: Option<f64>,
}

impl ResourceStatus {
    /// Check if resource is primary
    pub fn is_primary(&self) -> bool {
        self.role.to_lowercase() == "primary"
    }

    /// Check if resource is connected to all peers
    pub fn is_connected(&self) -> bool {
        self.connections
            .iter()
            .all(|c| c.connection_state.to_lowercase() == "connected")
    }

    /// Check if all disks are up-to-date
    pub fn is_uptodate(&self) -> bool {
        self.devices
            .iter()
            .all(|d| d.disk_state.to_lowercase() == "uptodate")
    }

    /// Check if syncing with any peer
    pub fn is_syncing(&self) -> bool {
        self.connections.iter().any(|c| {
            c.peer_devices.iter().any(|pd| {
                let state = pd.replication_state.to_lowercase();
                state.contains("sync") || state == "pausedsync"
            })
        })
    }

    /// Get sync progress as percentage (0-100)
    pub fn sync_progress(&self) -> Option<f64> {
        for conn in &self.connections {
            for pd in &conn.peer_devices {
                if pd.percent_in_sync.is_some() && pd.percent_in_sync != Some(100.0) {
                    return pd.percent_in_sync;
                }
            }
        }
        None
    }
}

/// DRBD command builder
pub struct DrbdCmd;

impl DrbdCmd {
    /// Get status command
    pub fn status_cmd() -> String {
        "drbdsetup status --json".to_string()
    }

    /// Get resource status command
    pub fn resource_status_cmd(resource: &str) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdsetup status {} --json", resource))
    }

    /// Create resource command (drbdadm create-md)
    pub fn create_md_cmd(resource: &str) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        // Use --force to skip confirmation prompts
        Ok(format!("drbdadm create-md --force {}", resource))
    }

    /// Adjust resource command (applies configuration)
    pub fn adjust_cmd(resource: &str) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm adjust {}", resource))
    }

    /// Up resource command
    pub fn up_cmd(resource: &str) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm up {}", resource))
    }

    /// Down resource command
    pub fn down_cmd(resource: &str) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm down {}", resource))
    }

    /// Primary command
    pub fn primary_cmd(resource: &str, force: bool) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        if force {
            Ok(format!("drbdadm primary {} --force", resource))
        } else {
            Ok(format!("drbdadm primary {}", resource))
        }
    }

    /// Secondary command
    pub fn secondary_cmd(resource: &str) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm secondary {}", resource))
    }

    /// Connect command
    pub fn connect_cmd(resource: &str) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm connect {}", resource))
    }

    /// Disconnect command
    pub fn disconnect_cmd(resource: &str) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm disconnect {}", resource))
    }

    /// Invalidate command (start resync from peer)
    pub fn invalidate_cmd(resource: &str) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm invalidate {}", resource))
    }

    /// Verify command
    pub fn verify_cmd(resource: &str) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm verify {}", resource))
    }

    /// Connect with discard-my-data (for split brain recovery as victim)
    pub fn connect_discard_cmd(resource: &str) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm connect --discard-my-data {}", resource))
    }

    /// Pause sync command
    pub fn pause_sync_cmd(resource: &str) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm pause-sync {}", resource))
    }

    /// Resume sync command
    pub fn resume_sync_cmd(resource: &str) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm resume-sync {}", resource))
    }

    /// Resize command
    pub fn resize_cmd(resource: &str) -> AppResult<String> {
        validator::validate_resource_name(resource)?;
        Ok(format!("drbdadm resize {}", resource))
    }

    /// Dump configuration
    pub fn dump_cmd(resource: &str) -> AppResult<String> {
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

    /// Create filesystem on DRBD device
    pub fn mkfs_cmd(device: &str, fstype: &str) -> AppResult<String> {
        validator::validate_block_device(device)?;
        // Validate fstype
        let allowed_fstypes = ["ext4", "xfs", "btrfs"];
        if !allowed_fstypes.contains(&fstype) {
            return Err(AppError::Validation(format!(
                "Invalid filesystem type '{}'. Allowed: {:?}",
                fstype, allowed_fstypes
            )));
        }
        Ok(format!("mkfs.{} {}", fstype, device))
    }

    /// Mount DRBD device
    pub fn mount_cmd(device: &str, mount_point: &str) -> AppResult<String> {
        validator::validate_block_device(device)?;
        validator::validate_mount_point(mount_point)?;
        Ok(format!("mount {} {}", device, mount_point))
    }

    /// Unmount DRBD device
    pub fn umount_cmd(mount_point: &str) -> AppResult<String> {
        validator::validate_mount_point(mount_point)?;
        Ok(format!("umount {}", mount_point))
    }

    /// Create mount point directory
    pub fn mkdir_cmd(mount_point: &str) -> AppResult<String> {
        validator::validate_mount_point(mount_point)?;
        Ok(format!("mkdir -p {}", mount_point))
    }
}

/// Parse DRBD status JSON output
pub fn parse_drbd_status(json_output: &str) -> AppResult<Vec<ResourceStatus>> {
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

    Err(AppError::Drbd(format!(
        "Failed to parse DRBD status JSON: {}",
        json_output
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drbd_commands() {
        assert!(DrbdCmd::up_cmd("r0").is_ok());
        assert!(DrbdCmd::up_cmd("my-resource").is_ok());
        assert!(DrbdCmd::up_cmd("bad;name").is_err());
        assert!(DrbdCmd::up_cmd("").is_err());
    }

    #[test]
    fn test_mkfs_validation() {
        assert!(DrbdCmd::mkfs_cmd("/dev/drbd0", "ext4").is_ok());
        assert!(DrbdCmd::mkfs_cmd("/dev/drbd0", "xfs").is_ok());
        assert!(DrbdCmd::mkfs_cmd("/dev/drbd0", "ntfs").is_err());
        assert!(DrbdCmd::mkfs_cmd("/etc/passwd", "ext4").is_err());
    }

    #[test]
    fn test_parse_drbd_status() {
        let json = r#"[{"name":"r0","role":"Primary","devices":[{"volume":0,"disk-state":"UpToDate","minor":0}],"connections":[]}]"#;
        let status = parse_drbd_status(json).unwrap();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].name, "r0");
        assert!(status[0].is_primary());
        assert!(status[0].is_uptodate());
    }

    #[test]
    fn test_resource_status_methods() {
        let status = ResourceStatus {
            name: "r0".to_string(),
            role: "Primary".to_string(),
            devices: vec![DeviceStatus {
                volume: 0,
                disk_state: "UpToDate".to_string(),
                minor: 0,
                size: Some(1073741824),
            }],
            connections: vec![ConnectionStatus {
                peer_node_id: 1,
                name: "node2".to_string(),
                connection_state: "Connected".to_string(),
                peer_devices: vec![PeerDeviceStatus {
                    volume: 0,
                    replication_state: "Established".to_string(),
                    peer_disk_state: "UpToDate".to_string(),
                    percent_in_sync: Some(100.0),
                }],
            }],
        };

        assert!(status.is_primary());
        assert!(status.is_connected());
        assert!(status.is_uptodate());
        assert!(!status.is_syncing());
        assert!(status.sync_progress().is_none());
    }
}

//! Cluster and node data models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A node in the cluster
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Node {
    /// Unique node identifier
    pub id: String,
    /// Node hostname
    pub hostname: String,
    /// Node IP address
    pub ip: String,
    /// SSH port
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    /// SSH username
    #[serde(default = "default_ssh_user")]
    pub ssh_user: String,
    /// Whether this is the local node
    #[serde(default)]
    pub is_local: bool,
    /// Node status
    #[serde(default)]
    pub status: NodeStatus,
    /// Last status detail, typically for connection/sudo validation failures
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    /// Last successful connection time
    pub last_seen: Option<DateTime<Utc>>,
}

fn default_ssh_port() -> u16 {
    22
}

fn default_ssh_user() -> String {
    "root".to_string()
}

/// Node connection status
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    #[default]
    Unknown,
    Online,
    Offline,
    Error,
}

/// Request to add a new node
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddNodeRequest {
    /// Node hostname
    pub hostname: String,
    /// Node IP address
    pub ip: String,
    /// SSH port (optional, defaults to 22)
    pub ssh_port: Option<u16>,
    /// SSH username (optional, defaults to "root")
    pub ssh_user: Option<String>,
}

/// Block device information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BlockDevice {
    pub name: String,
    pub path: Option<String>,
    pub size: u64, // Size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_human: Option<String>, // Human readable size (computed)
    #[serde(rename = "type")]
    pub device_type: String,
    pub mountpoint: Option<String>,
    pub fstype: Option<String>,
    #[serde(default)]
    pub ro: bool, // Read-only
    pub model: Option<String>,
    // Use explicit recursion handling for children if present, or ignore for docs to be safe
    #[serde(default)]
    #[schema(no_recursion)]
    pub children: Vec<BlockDevice>,
}

impl BlockDevice {
    /// Check if this device is available for DRBD use
    /// (no filesystem, not mounted, not a partition with children)
    pub fn is_available(&self) -> bool {
        self.fstype.is_none()
            && self.mountpoint.is_none()
            && self.children.is_empty()
            && !self.ro
            && (self.device_type == "disk"
                || self.device_type == "part"
                || self.device_type == "lvm")
    }

    /// Get human-readable size
    pub fn size_human(&self) -> String {
        human_readable_size(self.size)
    }
}

/// Convert bytes to human-readable size
fn human_readable_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// lsblk JSON output structure
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LsblkOutput {
    pub blockdevices: Vec<BlockDevice>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_device_available() {
        let dev = BlockDevice {
            name: "sdb".to_string(),
            path: Some("/dev/sdb".to_string()),
            size: 107374182400,
            size_human: Some("100G".to_string()),
            device_type: "disk".to_string(),
            mountpoint: None,
            fstype: None,
            ro: false,
            model: Some("VBOX HARDDISK".to_string()),
            children: vec![],
        };
        assert!(dev.is_available());

        let mounted = BlockDevice {
            mountpoint: Some("/mnt".to_string()),
            ..dev.clone()
        };
        assert!(!mounted.is_available());

        let with_fs = BlockDevice {
            fstype: Some("ext4".to_string()),
            ..dev.clone()
        };
        assert!(!with_fs.is_available());
    }

    #[test]
    fn test_parse_lsblk_json() {
        let json = r#"{
            "blockdevices": [
                {
                    "name": "sda",
                    "path": "/dev/sda",
                    "size": 53687091200,
                    "type": "disk",
                    "mountpoint": null,
                    "fstype": null,
                    "ro": false,
                    "model": "VBOX HARDDISK",
                    "children": [
                        {
                            "name": "sda1",
                            "path": "/dev/sda1",
                            "size": 53685993984,
                            "type": "part",
                            "mountpoint": "/",
                            "fstype": "ext4",
                            "ro": false
                        }
                    ]
                },
                {
                    "name": "sdb",
                    "path": "/dev/sdb",
                    "size": 10737418240,
                    "type": "disk",
                    "mountpoint": null,
                    "fstype": null,
                    "ro": false,
                    "model": "VBOX HARDDISK",
                    "children": []
                }
            ]
        }"#;

        let output: LsblkOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.blockdevices.len(), 2);

        // sda has children (partitions) so not available
        assert!(!output.blockdevices[0].is_available());
        // sdb is clean and available
        assert!(output.blockdevices[1].is_available());
    }
}

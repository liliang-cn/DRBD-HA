//! DRBD synchronization status checking utilities
//!
//! This module provides reliable methods to check if DRBD resources are fully
//! synchronized between all nodes in a cluster.

use crate::error::DrbdResult;
use crate::models::{DrbdResourceStatus, ResourceStatus};
use shell_cmd::run_shell_command;

/// Check if DRBD resource is fully synced between all nodes
///
/// This function uses multiple methods to determine sync status:
/// 1. JSON output from drbdsetup (preferred for reliability)
/// 2. Text output parsing as fallback
///
/// Returns true if:
/// - Local disk is UpToDate
/// - All peer disks are UpToDate
/// - No sync operations are in progress
/// - All connections are established
pub async fn check_drbd_sync_complete(resource_name: &str) -> DrbdResult<bool> {
    // Method 1: Try JSON output first (most reliable)
    if let Ok(json_cmd) = crate::cmd::DrbdCmd::resource_status_cmd(resource_name) {
        if let Ok(output) = run_shell_command(&json_cmd, "Get DRBD JSON status").await {
            if output.success() {
                if let Ok(status) = crate::parse_drbd_status(&output.stdout) {
                    return Ok(is_fully_synced_json(&status, resource_name));
                }
            }
        }
    } else {
        tracing::debug!("Failed to generate DRBD JSON command, falling back to text parsing");
    }

    // Method 2: Fallback to text parsing
    check_drbd_sync_complete_text(resource_name).await
}

/// Helper function to check sync status using text output parsing
async fn check_drbd_sync_complete_text(resource_name: &str) -> DrbdResult<bool> {
    let cmd = format!("drbdadm status {}", resource_name);
    let output = run_shell_command(&cmd, "Get DRBD text status").await?;

    // Parse the status using drbd-utils parser
    if let Some(status) = crate::parse_drbdadm_status(&output.stdout, resource_name) {
        return Ok(is_fully_synced_text(&status));
    }

    // Fallback to simple string matching (legacy method)
    let is_synced = output.stdout.contains("peer-disk:UpToDate")
        && !output.stdout.contains("Inconsistent")
        && !output.stdout.contains("SyncSource")
        && !output.stdout.contains("SyncTarget")
        && !output.stdout.contains("StartingSync")
        && !output.stdout.contains("WFConnection");

    Ok(is_synced)
}

/// Check if DRBD resource is fully synced from JSON status
fn is_fully_synced_json(status: &[ResourceStatus], resource_name: &str) -> bool {
    if let Some(resource) = status.iter().find(|r| r.name == resource_name) {
        return resource.is_uptodate() && resource.is_connected() && !resource.is_syncing();
    }
    false
}

/// Check if DRBD resource is fully synced from parsed text status
fn is_fully_synced_text(status: &DrbdResourceStatus) -> bool {
    // Local disk must be UpToDate
    if status.disk != "UpToDate" {
        return false;
    }

    // All peer disks must be UpToDate and connected
    for peer in &status.peers {
        if peer.peer_disk != "UpToDate" {
            return false;
        }

        if let Some(ref connection) = peer.connection {
            if connection != "Connected" {
                return false;
            }
        }
    }

    true
}

/// Get detailed sync status information for a DRBD resource
///
/// Returns comprehensive information about the sync status including:
/// - Local role and disk state
/// - Peer information
/// - Sync progress (if in progress)
/// - Connection status
pub async fn get_drbd_sync_status(resource_name: &str) -> DrbdResult<SyncStatus> {
    // Try to get JSON status first
    if let Ok(json_cmd) = crate::cmd::DrbdCmd::resource_status_cmd(resource_name) {
        if let Ok(output) = run_shell_command(&json_cmd, "Get DRBD JSON status").await {
            if output.success() {
                if let Ok(status) = crate::parse_drbd_status(&output.stdout) {
                    if let Some(resource) = status.iter().find(|r| r.name == resource_name) {
                        return Ok(SyncStatus::from_json_resource(resource));
                    }
                }
            }
        }
    } else {
        tracing::debug!("JSON status failed, trying text parsing");
    }

    // Fallback to text parsing
    let cmd = format!("drbdadm status {}", resource_name);
    let output = run_shell_command(&cmd, "Get DRBD text status").await?;

    if let Some(status) = crate::parse_drbdadm_status(&output.stdout, resource_name) {
        Ok(SyncStatus::from_text_status(&status))
    } else {
        Err(crate::error::DrbdError::Validation(format!(
            "Could not parse DRBD status for resource: {}",
            resource_name
        )))
    }
}

/// Detailed sync status information
#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub resource_name: String,
    pub local_role: String,
    pub local_disk_state: String,
    pub is_fully_synced: bool,
    pub is_syncing: bool,
    pub sync_progress_percent: Option<f64>,
    pub peers: Vec<PeerSyncStatus>,
}

/// Sync status for a peer node
#[derive(Debug, Clone)]
pub struct PeerSyncStatus {
    pub name: String,
    pub role: String,
    pub disk_state: String,
    pub connection_state: String,
    pub replication_state: String,
}

impl SyncStatus {
    fn from_json_resource(resource: &ResourceStatus) -> Self {
        let peers = resource
            .connections
            .iter()
            .map(|conn| PeerSyncStatus {
                name: conn.name.clone(),
                role: conn.peer_role.as_deref().unwrap_or("Unknown").to_string(),
                disk_state: conn
                    .peer_devices
                    .first()
                    .map(|d| d.peer_disk_state.clone())
                    .unwrap_or_else(|| "Unknown".to_string()),
                connection_state: conn.connection_state.clone(),
                replication_state: conn
                    .peer_devices
                    .first()
                    .map(|d| d.replication_state.clone())
                    .unwrap_or_else(|| "Unknown".to_string()),
            })
            .collect();

        let is_syncing = resource.is_syncing();
        let sync_progress_percent = resource.sync_progress();

        Self {
            resource_name: resource.name.clone(),
            local_role: resource.role.clone(),
            local_disk_state: resource
                .devices
                .first()
                .map(|d| d.disk_state.clone())
                .unwrap_or_else(|| "Unknown".to_string()),
            is_fully_synced: resource.is_uptodate() && resource.is_connected() && !is_syncing,
            is_syncing,
            sync_progress_percent,
            peers,
        }
    }

    fn from_text_status(status: &DrbdResourceStatus) -> Self {
        let peers = status
            .peers
            .iter()
            .map(|peer| PeerSyncStatus {
                name: peer.name.clone(),
                role: peer.role.clone(),
                disk_state: peer.peer_disk.clone(),
                connection_state: peer.connection.as_deref().unwrap_or("Unknown").to_string(),
                replication_state: peer.replication.as_deref().unwrap_or("Unknown").to_string(),
            })
            .collect();

        let is_fully_synced = is_fully_synced_text(status);
        let is_syncing = status.peers.iter().any(|p| {
            p.peer_disk == "Inconsistent"
                || p.connection.as_ref().is_some_and(|c| c.contains("Sync"))
        });

        let sync_progress_percent = status.peers.iter().find_map(|p| p.sync_percent);

        Self {
            resource_name: status.resource.clone(),
            local_role: status.role.clone(),
            local_disk_state: status.disk.clone(),
            is_fully_synced,
            is_syncing,
            sync_progress_percent,
            peers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::*;

    #[test]
    fn test_is_fully_synced_json_parsing() {
        // Test with fully synced status
        let fully_synced = vec![ResourceStatus {
            name: "test-resource".to_string(),
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
                peer_role: Some("Secondary".to_string()),
                peer_devices: vec![PeerDeviceStatus {
                    volume: 0,
                    replication_state: "Established".to_string(),
                    peer_disk_state: "UpToDate".to_string(),
                    percent_in_sync: Some(100.0),
                }],
            }],
        }];

        assert!(is_fully_synced_json(&fully_synced, "test-resource"));

        // Test with syncing status
        let syncing = vec![ResourceStatus {
            name: "test-resource".to_string(),
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
                peer_role: Some("Secondary".to_string()),
                peer_devices: vec![PeerDeviceStatus {
                    volume: 0,
                    replication_state: "SyncTarget".to_string(),
                    peer_disk_state: "Inconsistent".to_string(),
                    percent_in_sync: Some(75.5),
                }],
            }],
        }];

        assert!(!is_fully_synced_json(&syncing, "test-resource"));
    }

    #[test]
    fn test_is_fully_synced_text_parsing() {
        // Test with fully synced status
        let synced_status = DrbdResourceStatus {
            resource: "mysql_data".to_string(),
            role: "Primary".to_string(),
            disk: "UpToDate".to_string(),
            open: true,
            minor: None,
            peers: vec![DrbdPeerStatus {
                name: "node2".to_string(),
                role: "Secondary".to_string(),
                peer_disk: "UpToDate".to_string(),
                connection: Some("Connected".to_string()),
                replication: Some("Established".to_string()),
                sync_percent: None,
            }],
        };
        assert!(is_fully_synced_text(&synced_status));

        // Test with syncing status
        let syncing_status = DrbdResourceStatus {
            resource: "mysql_data".to_string(),
            role: "Primary".to_string(),
            disk: "UpToDate".to_string(),
            open: true,
            minor: None,
            peers: vec![DrbdPeerStatus {
                name: "node2".to_string(),
                role: "Secondary".to_string(),
                peer_disk: "Inconsistent".to_string(),
                connection: Some("Connected".to_string()),
                replication: Some("SyncTarget".to_string()),
                sync_percent: Some(45.0),
            }],
        };
        assert!(!is_fully_synced_text(&syncing_status));
    }

    #[test]
    fn test_sync_status_from_json() {
        let resource = ResourceStatus {
            name: "test-resource".to_string(),
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
                peer_role: Some("Secondary".to_string()),
                peer_devices: vec![PeerDeviceStatus {
                    volume: 0,
                    replication_state: "Established".to_string(),
                    peer_disk_state: "UpToDate".to_string(),
                    percent_in_sync: Some(100.0),
                }],
            }],
        };

        let sync_status = SyncStatus::from_json_resource(&resource);
        assert_eq!(sync_status.resource_name, "test-resource");
        assert_eq!(sync_status.local_role, "Primary");
        assert_eq!(sync_status.local_disk_state, "UpToDate");
        assert!(sync_status.is_fully_synced);
        assert!(!sync_status.is_syncing);
        assert_eq!(sync_status.peers.len(), 1);
        assert_eq!(sync_status.peers[0].name, "node2");
        assert_eq!(sync_status.peers[0].disk_state, "UpToDate");
    }

    #[test]
    fn test_is_fully_synced_text_edge_cases() {
        // Test with no peers
        let no_peers = DrbdResourceStatus {
            resource: "test".to_string(),
            role: "Primary".to_string(),
            disk: "UpToDate".to_string(),
            open: true,
            minor: None,
            peers: vec![],
        };
        assert!(is_fully_synced_text(&no_peers));

        // Test with local disk not up to date
        let local_not_uptodate = DrbdResourceStatus {
            resource: "test".to_string(),
            role: "Primary".to_string(),
            disk: "Inconsistent".to_string(),
            open: true,
            minor: None,
            peers: vec![],
        };
        assert!(!is_fully_synced_text(&local_not_uptodate));
    }
}

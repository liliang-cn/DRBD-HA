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

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
    #[serde(rename = "peer-role", default)]
    pub peer_role: Option<String>,
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

/// Detailed DRBD resource status (from drbdadm status)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DrbdResourceStatus {
    /// Resource name
    pub resource: String,
    /// Local role (Primary/Secondary)
    pub role: String,
    /// Local disk state (UpToDate/Inconsistent/DUnknown etc)
    pub disk: String,
    /// Whether the device is open (mounted)
    pub open: bool,
    /// Peer node statuses
    pub peers: Vec<DrbdPeerStatus>,
}

/// Status of a DRBD peer node (from drbdadm status)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DrbdPeerStatus {
    /// Peer hostname
    pub name: String,
    /// Peer role (Primary/Secondary)
    pub role: String,
    /// Peer disk state
    pub peer_disk: String,
    /// Connection state (Connected/Connecting etc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    /// Replication state (Established/SyncSource/SyncTarget etc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replication: Option<String>,
}

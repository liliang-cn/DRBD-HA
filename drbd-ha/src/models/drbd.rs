//! DRBD data models
//!
//! This module contains data structures for representing DRBD resource status.
//! These structures are designed to parse the JSON output of `drbdsetup status --json`.
//!
//! Portions of this file are derived from drbd-reactor
//! Copyright (c) LINBIT HA-Solutions GmbH
//! https://github.com/LINBIT/drbd-reactor
//! SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use utoipa::ToSchema;

/// DRBD Resource - represents a complete DRBD resource with all its components
#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Resource {
    /// Resource name
    pub name: String,
    /// Current role (Primary/Secondary/Unknown)
    pub role: Role,
    /// Whether IO is suspended
    #[serde(default)]
    pub suspended: bool,
    /// Write ordering mode
    #[serde(default)]
    pub write_ordering: String,
    /// Whether IO failures are being forced
    #[serde(default)]
    pub force_io_failures: bool,
    /// Whether this node may be promoted to Primary
    #[serde(default)]
    pub may_promote: bool,
    /// Promotion score for automatic failover
    #[serde(default)]
    pub promotion_score: i32,
    /// Local devices (volumes)
    #[serde(default)]
    pub devices: Vec<Device>,
    /// Connections to peer nodes
    #[serde(default)]
    pub connections: Vec<Connection>,
}

impl Resource {
    /// Check if all connections are in Connected state
    pub fn is_connected(&self) -> bool {
        self.connections
            .iter()
            .all(|c| c.connection == ConnectionState::Connected)
    }

    /// Check if all devices are UpToDate
    pub fn is_up_to_date(&self) -> bool {
        self.devices
            .iter()
            .all(|d| d.disk_state == DiskState::UpToDate)
    }

    /// Get device by volume number
    pub fn get_device(&self, volume: i32) -> Option<&Device> {
        self.devices.iter().find(|d| d.volume == volume)
    }

    /// Get connection by peer node ID
    pub fn get_connection(&self, peer_node_id: i32) -> Option<&Connection> {
        self.connections
            .iter()
            .find(|c| c.peer_node_id == peer_node_id)
    }
}

/// Backing device path (can be None for diskless)
#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, ToSchema)]
pub struct BackingDevice(pub Option<String>);

impl FromStr for BackingDevice {
    type Err = std::io::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "none" => Ok(Self(None)),
            _ => Ok(Self(Some(input.to_string()))),
        }
    }
}

impl fmt::Display for BackingDevice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.0 {
            Some(bd) => write!(f, "{}", bd),
            None => write!(f, "none"),
        }
    }
}

/// DRBD Device (volume) - represents a local block device
#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Device {
    /// Device name (e.g., "drbd0")
    #[serde(default)]
    pub name: String,
    /// Volume number
    pub volume: i32,
    /// Minor device number
    #[serde(default)]
    pub minor: i32,
    /// Current disk state
    #[serde(default)]
    pub disk_state: DiskState,
    /// Backing block device path
    #[serde(default)]
    pub backing_dev: BackingDevice,
    /// Whether this is a client (diskless intentionally)
    #[serde(default)]
    pub client: bool,
    /// Whether quorum is established
    #[serde(default)]
    pub quorum: bool,
    /// Size in bytes
    #[serde(default)]
    pub size: u64,
    /// Bytes read
    #[serde(default)]
    pub read: u64,
    /// Bytes written
    #[serde(default)]
    pub written: u64,
    /// Activity log writes
    #[serde(default)]
    pub al_writes: u64,
    /// Bitmap writes
    #[serde(default)]
    pub bm_writes: u64,
    /// Upper pending IOs
    #[serde(default)]
    pub upper_pending: u64,
    /// Lower pending IOs
    #[serde(default)]
    pub lower_pending: u64,
    /// Whether activity log is suspended
    #[serde(default)]
    pub al_suspended: bool,
    /// Blocked reason (empty if not blocked)
    #[serde(default)]
    pub blocked: String,
    /// Whether device is open
    #[serde(default)]
    pub open: bool,
}

/// Peer device - represents a volume on a peer node
#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub struct PeerDevice {
    /// Device name
    #[serde(default)]
    pub name: String,
    /// Volume number
    pub volume: i32,
    /// Peer node ID
    pub peer_node_id: i32,
    /// Replication state
    #[serde(default)]
    pub replication_state: ReplicationState,
    /// Connection name
    #[serde(default)]
    pub conn_name: String,
    /// Peer's disk state
    #[serde(default)]
    pub peer_disk_state: DiskState,
    /// Whether peer is a client
    #[serde(default)]
    pub peer_client: bool,
    /// Whether resync is suspended
    #[serde(default)]
    pub resync_suspended: bool,
    /// Bytes received
    #[serde(default)]
    pub received: u64,
    /// Bytes sent
    #[serde(default)]
    pub sent: u64,
    /// Out of sync bytes
    #[serde(default)]
    pub out_of_sync: u64,
    /// Pending requests
    #[serde(default)]
    pub pending: u64,
    /// Unacknowledged requests
    #[serde(default)]
    pub unacked: u64,
    /// Whether sync details are available
    #[serde(default)]
    pub has_sync_details: bool,
    /// Whether online verify details are available
    #[serde(default)]
    pub has_online_verify_details: bool,
}

/// Connection to a peer node
#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Connection {
    /// Connection name (peer hostname)
    pub name: String,
    /// Peer node ID
    pub peer_node_id: i32,
    /// Connection name in config
    #[serde(default)]
    pub conn_name: String,
    /// Connection state
    #[serde(default)]
    pub connection: ConnectionState,
    /// Peer's role
    #[serde(default)]
    pub peer_role: Role,
    /// Whether connection is congested
    #[serde(default)]
    pub congested: bool,
    /// Application requests in flight
    #[serde(default)]
    pub ap_in_flight: u64,
    /// Resync requests in flight
    #[serde(default)]
    pub rs_in_flight: u64,
    /// Peer devices on this connection
    #[serde(default)]
    pub peerdevices: Vec<PeerDevice>,
    /// Network paths
    #[serde(default)]
    pub paths: Vec<Path>,
}

/// Network path to a peer
#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Path {
    /// Path name
    #[serde(default)]
    pub name: String,
    /// Peer node ID
    pub peer_node_id: i32,
    /// Connection name
    #[serde(default)]
    pub conn_name: String,
    /// Local address
    #[serde(default)]
    pub local: String,
    /// Peer address
    #[serde(default)]
    pub peer: String,
    /// Whether path is established
    #[serde(default)]
    pub established: bool,
}

/// DRBD Role
#[derive(Serialize, Deserialize, Hash, Debug, PartialEq, Eq, Clone, Default, ToSchema)]
pub enum Role {
    #[default]
    Unknown,
    Primary,
    Secondary,
}

impl FromStr for Role {
    type Err = std::io::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "Unknown" => Ok(Self::Unknown),
            "Primary" => Ok(Self::Primary),
            "Secondary" => Ok(Self::Secondary),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown role: {}", input),
            )),
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Unknown => write!(f, "Unknown"),
            Self::Primary => write!(f, "Primary"),
            Self::Secondary => write!(f, "Secondary"),
        }
    }
}

/// DRBD Disk State
#[derive(Serialize, Deserialize, Hash, Debug, Clone, PartialEq, Eq, Default, ToSchema)]
pub enum DiskState {
    Diskless,
    Attaching,
    Detaching,
    Failed,
    Negotiating,
    Inconsistent,
    Outdated,
    #[default]
    DUnknown,
    Consistent,
    UpToDate,
}

impl FromStr for DiskState {
    type Err = std::io::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "Diskless" => Ok(Self::Diskless),
            "Attaching" => Ok(Self::Attaching),
            "Detaching" => Ok(Self::Detaching),
            "Failed" => Ok(Self::Failed),
            "Negotiating" => Ok(Self::Negotiating),
            "Inconsistent" => Ok(Self::Inconsistent),
            "Outdated" => Ok(Self::Outdated),
            "DUnknown" => Ok(Self::DUnknown),
            "Consistent" => Ok(Self::Consistent),
            "UpToDate" => Ok(Self::UpToDate),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown disk state: {}", input),
            )),
        }
    }
}

impl fmt::Display for DiskState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Diskless => write!(f, "Diskless"),
            Self::Attaching => write!(f, "Attaching"),
            Self::Detaching => write!(f, "Detaching"),
            Self::Failed => write!(f, "Failed"),
            Self::Negotiating => write!(f, "Negotiating"),
            Self::Inconsistent => write!(f, "Inconsistent"),
            Self::Outdated => write!(f, "Outdated"),
            Self::DUnknown => write!(f, "DUnknown"),
            Self::Consistent => write!(f, "Consistent"),
            Self::UpToDate => write!(f, "UpToDate"),
        }
    }
}

/// DRBD Connection State
#[derive(Serialize, Deserialize, Hash, Debug, Clone, PartialEq, Eq, Default, ToSchema)]
pub enum ConnectionState {
    #[default]
    StandAlone,
    Disconnecting,
    Unconnected,
    Timeout,
    BrokenPipe,
    NetworkFailure,
    ProtocolError,
    TearDown,
    Connecting,
    Connected,
}

impl FromStr for ConnectionState {
    type Err = std::io::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "StandAlone" => Ok(Self::StandAlone),
            "Disconnecting" => Ok(Self::Disconnecting),
            "Unconnected" => Ok(Self::Unconnected),
            "Timeout" => Ok(Self::Timeout),
            "BrokenPipe" => Ok(Self::BrokenPipe),
            "NetworkFailure" => Ok(Self::NetworkFailure),
            "ProtocolError" => Ok(Self::ProtocolError),
            "TearDown" => Ok(Self::TearDown),
            "Connecting" => Ok(Self::Connecting),
            "Connected" => Ok(Self::Connected),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown connection state: {}", input),
            )),
        }
    }
}

impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::StandAlone => write!(f, "StandAlone"),
            Self::Disconnecting => write!(f, "Disconnecting"),
            Self::Unconnected => write!(f, "Unconnected"),
            Self::Timeout => write!(f, "Timeout"),
            Self::BrokenPipe => write!(f, "BrokenPipe"),
            Self::NetworkFailure => write!(f, "NetworkFailure"),
            Self::ProtocolError => write!(f, "ProtocolError"),
            Self::TearDown => write!(f, "TearDown"),
            Self::Connecting => write!(f, "Connecting"),
            Self::Connected => write!(f, "Connected"),
        }
    }
}

/// DRBD Replication State
#[derive(Serialize, Deserialize, Hash, Debug, Clone, PartialEq, Eq, Default, ToSchema)]
pub enum ReplicationState {
    #[default]
    Off,
    Established,
    StartingSyncS,
    StartingSyncT,
    WFBitMapS,
    WFBitMapT,
    WFSyncUUID,
    SyncSource,
    SyncTarget,
    VerifyS,
    VerifyT,
    PausedSyncS,
    PausedSyncT,
    Ahead,
    Behind,
}

impl FromStr for ReplicationState {
    type Err = std::io::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "Off" => Ok(Self::Off),
            "Established" => Ok(Self::Established),
            "StartingSyncS" => Ok(Self::StartingSyncS),
            "StartingSyncT" => Ok(Self::StartingSyncT),
            "WFBitMapS" => Ok(Self::WFBitMapS),
            "WFBitMapT" => Ok(Self::WFBitMapT),
            "WFSyncUUID" => Ok(Self::WFSyncUUID),
            "SyncSource" => Ok(Self::SyncSource),
            "SyncTarget" => Ok(Self::SyncTarget),
            "VerifyS" => Ok(Self::VerifyS),
            "VerifyT" => Ok(Self::VerifyT),
            "PausedSyncS" => Ok(Self::PausedSyncS),
            "PausedSyncT" => Ok(Self::PausedSyncT),
            "Ahead" => Ok(Self::Ahead),
            "Behind" => Ok(Self::Behind),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown replication state: {}", input),
            )),
        }
    }
}

impl fmt::Display for ReplicationState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::Established => write!(f, "Established"),
            Self::StartingSyncS => write!(f, "StartingSyncS"),
            Self::StartingSyncT => write!(f, "StartingSyncT"),
            Self::WFBitMapS => write!(f, "WFBitMapS"),
            Self::WFBitMapT => write!(f, "WFBitMapT"),
            Self::WFSyncUUID => write!(f, "WFSyncUUID"),
            Self::SyncSource => write!(f, "SyncSource"),
            Self::SyncTarget => write!(f, "SyncTarget"),
            Self::VerifyS => write!(f, "VerifyS"),
            Self::VerifyT => write!(f, "VerifyT"),
            Self::PausedSyncS => write!(f, "PausedSyncS"),
            Self::PausedSyncT => write!(f, "PausedSyncT"),
            Self::Ahead => write!(f, "Ahead"),
            Self::Behind => write!(f, "Behind"),
        }
    }
}

/// DRBD version information
#[derive(Debug, Clone, Default, PartialEq, PartialOrd)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// DRBD kernel module and userspace utils versions
#[derive(Debug, Clone, Default)]
pub struct DrbdVersion {
    pub kmod: Version,
    pub utils: Version,
}

/// Request to create a new DRBD resource
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateResourceRequest {
    /// Resource name
    pub name: String,
    /// DRBD port number (7789-8000)
    pub port: u16,
    /// DRBD minor number
    pub minor: u32,
    /// Node IDs and their backing device paths
    /// Key: node_id, Value: backing device path (e.g., "/dev/sdb1")
    pub node_disks: std::collections::HashMap<String, String>,
    /// Whether to enable auto-promote (default: true)
    /// Set to false for resources managed by drbd-reactor
    #[serde(default = "default_auto_promote")]
    pub auto_promote: bool,
    /// Whether to force creation despite safety checks
    #[serde(default)]
    pub force: bool,
    /// Optional: Initialize disks as LVM PV/VG/LV before use
    #[serde(default)]
    pub init_lvm: bool,
    /// Optional: Name for the new Volume Group (required if init_lvm is true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lvm_vg_name: Option<String>,
    /// Optional: Name for the new Logical Volume (defaults to resource name if not set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lvm_lv_name: Option<String>,
    /// Optional: Size of the Logical Volume (e.g. "10G", defaults to 100%FREE)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lvm_lv_size: Option<String>,
    /// Optional: Storage pool type to use (lvm, zfs, or none for raw)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_type: Option<String>,
    /// Optional: Name for the ZFS pool (required if storage_type is "zfs")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zfs_pool_name: Option<String>,
    /// Optional: Name for the ZFS volume (defaults to resource name if not set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zfs_volume_name: Option<String>,
    /// Optional: Size of the ZFS volume in GB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zfs_volume_size_gb: Option<u64>,
}

fn default_auto_promote() -> bool {
    true
}

/// Request to create filesystem on DRBD device
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateFilesystemRequest {
    /// Filesystem type (ext4, xfs, btrfs)
    pub fstype: String,
    /// Whether to force creation (dangerous!)
    #[serde(default)]
    pub force: bool,
}

/// Request to mount DRBD device
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MountRequest {
    /// Mount point path
    pub mount_point: String,
    /// Mount options (optional)
    pub options: Option<String>,
}

/// Resource action request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResourceAction {
    Up,
    Down,
    Primary,
    Secondary,
    Connect,
    Disconnect,
    Invalidate,
    Verify,
    /// Recover from split brain as the victim (discard local data)
    /// This executes: disconnect -> secondary -> connect --discard-my-data
    RecoverSplitBrain,
}

/// Request to perform an action on a resource
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResourceActionRequest {
    /// The action to perform
    pub action: ResourceAction,
    /// Force the action (for primary --force)
    #[serde(default)]
    pub force: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_parsing() {
        assert_eq!(Role::from_str("Primary").unwrap(), Role::Primary);
        assert_eq!(Role::from_str("Secondary").unwrap(), Role::Secondary);
        assert_eq!(Role::from_str("Unknown").unwrap(), Role::Unknown);
        assert!(Role::from_str("Invalid").is_err());
    }

    #[test]
    fn test_disk_state_parsing() {
        assert_eq!(
            DiskState::from_str("UpToDate").unwrap(),
            DiskState::UpToDate
        );
        assert_eq!(
            DiskState::from_str("Inconsistent").unwrap(),
            DiskState::Inconsistent
        );
    }

    #[test]
    fn test_connection_state_parsing() {
        assert_eq!(
            ConnectionState::from_str("Connected").unwrap(),
            ConnectionState::Connected
        );
        assert_eq!(
            ConnectionState::from_str("StandAlone").unwrap(),
            ConnectionState::StandAlone
        );
    }

    #[test]
    fn test_resource_deserialize() {
        let json = r#"[{
            "name": "r0",
            "role": "Primary",
            "suspended": false,
            "write-ordering": "flush",
            "may-promote": true,
            "promotion-score": 10101,
            "devices": [{
                "volume": 0,
                "minor": 0,
                "disk-state": "UpToDate",
                "quorum": true
            }],
            "connections": [{
                "name": "node2",
                "peer-node-id": 1,
                "connection": "Connected",
                "peer-role": "Secondary"
            }]
        }]"#;

        let resources: Vec<Resource> = serde_json::from_str(json).unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].name, "r0");
        assert_eq!(resources[0].role, Role::Primary);
        assert!(resources[0].is_connected());
        assert!(resources[0].is_up_to_date());
    }
}

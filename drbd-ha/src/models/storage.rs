use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StoragePool {
    pub id: String,
    pub name: String,    // e.g., "ha_pool"
    pub node_id: String, // Node where this pool exists
    pub device: String,  // e.g., "/dev/sdb"
    pub total_size: u64,
    pub free_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Volume {
    pub id: String,      // Unique volume ID
    pub pool_id: String, // ID of the parent StoragePool
    pub name: String,    // e.g., "vol_mysql"
    pub size_gb: u64,
    pub device_path: String,      // e.g., "/dev/ha_pool/vol_mysql"
    pub drbd_res: Option<String>, // Associated DRBD resource name
}

/// Request to create a new storage pool (LVM VG)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateStoragePoolRequest {
    pub name: String, // e.g., "ha_pool"
    /// Primary device (legacy/single-node support).
    /// If `node_devices` is empty, this device is used for the local node.
    pub device: Option<String>,
    pub pool_type: String, // e.g., "lvm"

    /// Map of Node ID to Device Path for cluster-wide pool creation.
    /// e.g. {"node1": "/dev/sdb", "node2": "/dev/sdc"}
    #[serde(default)]
    pub node_devices: HashMap<String, String>,
}

/// Specification for a node and device pair
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeDeviceSpec {
    pub node_id: String, // Node where pool should be created
    pub device: String,  // e.g., "/dev/sdb" - Physical device to create VG on
}

/// Response for storage pool creation
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CreateStoragePoolResponse {
    pub id: String,
    pub name: String,
    pub node_id: String,
    pub device: String,
    pub total_size: u64,
    pub free_size: u64,
}

/// Response for listing storage pools
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ListStoragePoolResponse {
    pub pools: Vec<StoragePool>,
}

/// Request to create a new logical volume (LVM LV)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateVolumeRequest {
    pub pool_id: String, // ID of the parent StoragePool
    pub name: String,    // e.g., "vol_mysql"
    pub size_gb: u64,
}

/// Response for logical volume creation
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CreateVolumeResponse {
    pub id: String,
    pub name: String,
    pub pool_id: String,
    pub size_gb: u64,
    pub device_path: String,
}

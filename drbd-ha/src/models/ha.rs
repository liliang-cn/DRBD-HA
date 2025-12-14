//! High Availability profile data models

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// HA Profile Type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum HaType {
    #[default]
    Generic,
    Nfs,
    Iscsi,
    NvmeOf,
}

/// NFS Configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NfsConfig {
    pub export_path: String,           // e.g., "/exports/share1"
    pub allowed_networks: Vec<String>, // e.g., ["192.168.1.0/24"]
    pub options: String,               // e.g., "rw,sync,no_root_squash"
}

/// iSCSI Configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IscsiConfig {
    pub iqn: String, // e.g., "iqn.2025-01.com.haforge:lun1"
    pub allowed_initiators: Vec<String>,
    // TODO: CHAP auth
}

/// NVMe-oF Configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NvmeOfConfig {
    pub nqn: String, // e.g., "nqn.2025-01.com.haforge:subsys"
    pub allowed_nqns: Vec<String>,
    pub fabric_type: String, // e.g., "tcp"
    pub trsvcid: String,     // e.g., "4420"
}

/// HA Profile - defines how a service should be made highly available
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HaProfile {
    /// Unique profile ID
    pub id: String,
    /// Profile name
    pub name: String,
    /// Service Type
    #[serde(default)]
    pub ha_type: HaType,
    /// The DRBD resource this profile depends on
    pub resource_name: String,
    /// Mount point for the DRBD volume
    pub mount_point: String,
    /// Filesystem type (xfs, ext4, btrfs)
    #[serde(default = "default_fs_type")]
    pub fs_type: String,
    /// Virtual IP address (optional)
    pub vip: Option<VipConfig>,
    /// Promoter configuration
    pub promoter: PromoterSettings,
    /// Profile status
    #[serde(default)]
    pub status: HaProfileStatus,
    /// The node where the service is currently active
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_node: Option<String>,
    /// Generated systemd unit information
    #[serde(default)]
    pub generated_units: GeneratedUnits,

    // Specific configs
    pub nfs: Option<NfsConfig>,
    pub iscsi: Option<IscsiConfig>,
    pub nvmeof: Option<NvmeOfConfig>,
}

/// Promoter settings for drbd-reactor
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PromoterSettings {
    /// Services to start when promoted (in order)
    pub services: Vec<String>,
    /// Whether to stop services on demotion
    #[serde(default = "default_true")]
    pub stop_on_demote: bool,
    /// Action on demote failure: "reboot", "force", or "ignore"
    #[serde(default = "default_on_demote_failure")]
    pub on_demote_failure: String,
}

impl Default for PromoterSettings {
    fn default() -> Self {
        Self {
            services: Vec::new(),
            stop_on_demote: true,
            on_demote_failure: "reboot".to_string(),
        }
    }
}

fn default_on_demote_failure() -> String {
    "reboot".to_string()
}

/// Virtual IP configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VipConfig {
    /// IP address
    pub address: String,
    /// CIDR netmask (e.g., 24 for /24)
    pub netmask: u8,
    /// Network interface (e.g., "eth0")
    pub interface: String,
}

impl VipConfig {
    /// Format as "address/netmask" (e.g., "192.168.1.100/24")
    pub fn cidr(&self) -> String {
        format!("{}/{}", self.address, self.netmask)
    }
}

/// HA Profile status
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum HaProfileStatus {
    #[default]
    Unknown,
    /// Service is active on this node
    Active,
    /// Service is standby (ready to take over)
    Standby,
    /// Service is stopped
    Stopped,
    /// Error state
    Error,
}

/// Request to create an HA profile
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateHaProfileRequest {
    /// Profile name
    pub name: String,
    /// Service Type
    #[serde(default)]
    pub ha_type: HaType,
    /// DRBD resource to use
    pub resource_name: String,
    /// Mount point for the DRBD volume (Required for Generic/NFS, Ignored for iSCSI/NVMe-oF)
    pub mount_point: String,
    /// Filesystem type (xfs, ext4, btrfs)
    #[serde(default = "default_fs_type")]
    pub fs_type: String,
    /// Services to start when promoted (in order)
    pub services: Vec<String>,
    /// Virtual IP configuration (optional)
    pub vip: Option<VipConfig>,
    /// Whether to stop services on demotion
    #[serde(default = "default_true")]
    pub stop_on_demote: bool,
    /// Action on demote failure: "reboot", "force", or "ignore"
    #[serde(default = "default_on_demote_failure")]
    pub on_demote_failure: String,
    /// Whether to automatically disable the managed services (systemctl disable)
    /// This prevents services from starting before DRBD is mounted after reboot
    #[serde(default = "default_true")]
    pub auto_disable_services: bool,
    /// Whether to initialize the service data directory (e.g. mysql_install_db)
    #[serde(default)]
    pub init_service: bool,
    /// Data migration options
    #[serde(default)]
    pub migration: Option<DataMigrationOptions>,
    /// Optional: ID of the LVM storage pool to create the volume in
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lvm_pool_id: Option<String>,
    /// Optional: Desired size of the LVM volume in GB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lvm_volume_size_gb: Option<u64>,
    /// Optional: DRBD port (required for LVM auto-creation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drbd_port: Option<u16>,
    /// Optional: DRBD minor number (required for LVM auto-creation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drbd_minor: Option<u32>,

    // Specific configs
    pub nfs: Option<NfsConfig>,
    pub iscsi: Option<IscsiConfig>,
    pub nvmeof: Option<NvmeOfConfig>,
}

/// Options for migrating existing data to DRBD storage
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DataMigrationOptions {
    /// Source directory to migrate data from
    /// If different from mount_point, data will be copied from here
    pub source_path: Option<String>,
    /// Whether to format the DRBD device before migration
    #[serde(default = "default_true")]
    pub format_device: bool,
    /// Whether to migrate existing data from source_path or mount_point
    #[serde(default)]
    pub migrate_data: bool,
    /// Whether to preserve file permissions and ownership
    #[serde(default = "default_true")]
    pub preserve_permissions: bool,
}

impl Default for DataMigrationOptions {
    fn default() -> Self {
        Self {
            source_path: None,
            format_device: true,
            migrate_data: false,
            preserve_permissions: true,
        }
    }
}

/// Information about generated systemd units
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct GeneratedUnits {
    /// Generated mount unit name (e.g., "var-lib-mysql.mount")
    pub mount_unit: Option<String>,
    /// Path to the generated mount unit file
    pub mount_unit_path: Option<String>,
    /// DRBD device path (e.g., "/dev/drbd/by-res/mysql_data/0")
    pub drbd_device: Option<String>,
    /// Service override paths (keyed by service name)
    pub service_overrides: Vec<ServiceOverride>,
}

/// Information about a service override
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceOverride {
    /// Service name (e.g., "mysql.service")
    pub service_name: String,
    /// Path to the override directory
    pub override_dir: String,
    /// Path to the override file
    pub override_path: String,
}

/// drbd-reactor promoter configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PromoterConfig {
    /// Promoter ID (usually same as HA profile name)
    pub id: String,
    /// Resources configuration
    pub resources: PromoterResources,
}

/// Resources section of promoter config
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PromoterResources {
    /// DRBD resource name
    pub resource_name: String,
    /// Services to start when promoted
    pub start: Vec<String>,
    /// Whether to stop services on Secondary
    #[serde(default = "default_true")]
    pub stop_services_on_exit: bool,
}

fn default_true() -> bool {
    true
}

fn default_fs_type() -> String {
    "xfs".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vip_cidr() {
        let vip = VipConfig {
            address: "192.168.1.100".to_string(),
            netmask: 24,
            interface: "eth0".to_string(),
        };
        assert_eq!(vip.cidr(), "192.168.1.100/24");
    }

    #[test]
    fn test_create_ha_profile_request() {
        let json = r#"{
            "name": "nginx-ha",
            "resource_name": "r0",
            "mount_point": "/var/www/html",
            "services": ["nginx.service"],
            "vip": {
                "address": "192.168.1.100",
                "netmask": 24,
                "interface": "eth0"
            }
        }"#;

        let req: CreateHaProfileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "nginx-ha");
        assert_eq!(req.resource_name, "r0");
        assert!(req.vip.is_some());
        assert_eq!(req.vip.unwrap().address, "192.168.1.100");
    }
}

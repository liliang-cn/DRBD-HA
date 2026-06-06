//! High Availability profile data models

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// HA Profile Type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema, Default, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum HaType {
    #[default]
    Generic,
}

/// Ordered parameter entry (key-value pair with order preserved)
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, JsonSchema)]
pub struct ParamEntry {
    pub key: String,
    pub value: String,
}

impl ParamEntry {
    /// Convert from config_gen ParamEntry
    pub fn from_config_gen(entry: config_gen::ParamEntry) -> Self {
        ParamEntry {
            key: entry.key,
            value: entry.value,
        }
    }

    /// Convert to config_gen ParamEntry
    pub fn to_config_gen(&self) -> config_gen::ParamEntry {
        config_gen::ParamEntry {
            key: self.key.clone(),
            value: self.value.clone(),
        }
    }

    /// Convert a Vec from config_gen
    pub fn vec_from_config_gen(entries: Vec<config_gen::ParamEntry>) -> Vec<Self> {
        entries
            .into_iter()
            .map(|e| ParamEntry {
                key: e.key,
                value: e.value,
            })
            .collect()
    }

    /// Convert a Vec to config_gen
    pub fn vec_to_config_gen(entries: &[Self]) -> Vec<config_gen::ParamEntry> {
        entries
            .iter()
            .map(|e| config_gen::ParamEntry {
                key: e.key.clone(),
                value: e.value.clone(),
            })
            .collect()
    }
}

/// OCF Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct OcfAgentConfig {
    /// Agent name (e.g., "ocf:heartbeat:IPaddr2")
    pub name: String,
    /// Unique instance name (e.g., "r0_vip")
    pub instance_name: String,
    /// Agent parameters (order preserved as array)
    pub params: Vec<ParamEntry>,
}

impl OcfAgentConfig {
    /// Get a parameter value by key
    pub fn get_param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|p| p.key == key)
            .map(|p| p.value.as_str())
    }

    /// Check if a parameter exists
    pub fn has_param(&self, key: &str) -> bool {
        self.params.iter().any(|p| p.key == key)
    }

    /// Set a parameter value (updates if exists, appends if not)
    pub fn set_param(&mut self, key: String, value: String) {
        if let Some(existing) = self.params.iter_mut().find(|p| p.key == key) {
            existing.value = value;
        } else {
            self.params.push(ParamEntry { key, value });
        }
    }

    /// Remove a parameter by key
    pub fn remove_param(&mut self, key: &str) {
        self.params.retain(|p| p.key != key);
    }
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
    /// Mount strategy for storage (systemd or ocf)
    #[serde(default)]
    pub mount_strategy: MountStrategy,
    /// Virtual IP address (optional)
    pub vip: Option<VipConfig>,
    /// OCF Resource Agents (optional)
    #[serde(default)]
    pub ocf_agents: Vec<OcfAgentConfig>,
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
    /// Whether this is a built-in drbd-reactor plugin (e.g., prometheus.toml)
    /// Built-in plugins don't support expand/collapse for detailed status
    #[serde(default)]
    pub is_builtin_plugin: bool,
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
    /// Preferred nodes list (ordered by priority)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_nodes: Option<Vec<String>>,
    /// Preferred nodes policy: "always" or "start-only"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_nodes_policy: Option<String>,
    /// Sleep before promote factor (multiplier for promotion delay)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sleep_before_promote_factor: Option<u32>,
    /// Dependency type between services (Requires, Wants, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies_as: Option<String>,
    /// Target dependency type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_as: Option<String>,
    /// Action on quorum loss
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_quorum_loss: Option<String>,
}

impl Default for PromoterSettings {
    fn default() -> Self {
        Self {
            services: Vec::new(),
            stop_on_demote: true,
            on_demote_failure: "reboot".to_string(),
            preferred_nodes: None,
            preferred_nodes_policy: None,
            sleep_before_promote_factor: None,
            dependencies_as: None,
            target_as: None,
            on_quorum_loss: None,
        }
    }
}

fn default_on_demote_failure() -> String {
    "reboot".to_string()
}

/// Virtual IP configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct VipConfig {
    /// IP address
    pub address: String,
    /// CIDR netmask (e.g., 24 for /24)
    pub netmask: u8,
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
    /// Profile is disabled (.toml.disabled)
    Disabled,
}

/// Request to create an HA profile
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
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
    /// OCF Resource Agents (optional)
    #[serde(default)]
    pub ocf_agents: Vec<OcfAgentConfig>,
    /// Whether to stop services on demotion
    #[serde(default = "default_true")]
    pub stop_on_demote: bool,
    /// Action on demote failure: "reboot", "force", or "ignore"
    #[serde(default = "default_on_demote_failure")]
    pub on_demote_failure: String,
    /// Preferred nodes list (ordered by priority)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_nodes: Option<Vec<String>>,
    /// Preferred nodes policy: "always" or "start-only"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_nodes_policy: Option<String>,
    /// Sleep before promote factor (multiplier for promotion delay)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sleep_before_promote_factor: Option<u32>,
    /// Dependency type between services (Requires, Wants, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies_as: Option<String>,
    /// Target dependency type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_as: Option<String>,
    /// Action on quorum loss
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_quorum_loss: Option<String>,
    /// Mount strategy for the storage (systemd or ocf)
    #[serde(default)]
    pub mount_strategy: MountStrategy,
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
    /// Optional: Name of the LVM thin pool to use/create
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lvm_thin_pool_name: Option<String>,
    /// Optional: Size of the LVM thin pool metadata or total size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lvm_thin_pool_size: Option<String>,
    /// Optional: ID of the ZFS storage pool to create the volume in
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zfs_pool_id: Option<String>,
    /// Optional: Desired size of the ZFS volume in GB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zfs_volume_size_gb: Option<u64>,
    /// Optional: Map of node ID to raw disk path for storage pool initialization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_disks: Option<std::collections::HashMap<String, String>>,
    /// Optional: DRBD port (required for LVM auto-creation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drbd_port: Option<u16>,
    /// Optional: DRBD minor number (required for LVM auto-creation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drbd_minor: Option<u32>,
    /// Whether to create the profile in disabled state (.toml.disabled)
    /// This allows reviewing configuration before activation
    #[serde(default)]
    pub start_disabled: bool,
    /// Optional raw drbd-reactor promoter TOML configuration
    /// If provided, this will be used directly instead of generating from structured fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promoter_config_raw: Option<String>,
}

/// Options for migrating existing data to DRBD storage
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
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

/// Mount strategy for HA profiles
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default, PartialEq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MountStrategy {
    /// Use systemd mount units (recommended for most use cases)
    #[default]
    Systemd,
    /// Use OCF Filesystem agent (advanced HA features)
    Ocf,
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
    /// Path to the generated DRBD resource configuration file
    pub drbd_config_path: Option<String>,
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
                "netmask": 24
            }
        }"#;

        let req: CreateHaProfileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "nginx-ha");
        assert_eq!(req.resource_name, "r0");
        assert!(req.vip.is_some());
        assert_eq!(req.vip.unwrap().address, "192.168.1.100");
    }
}

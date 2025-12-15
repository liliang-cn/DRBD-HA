use crate::core::systemd_ctrl::{ServiceFileInfo, ServiceInfo};
use crate::models::{GeneratedUnits, HaProfile, HaProfileStatus};
use drbd_utils::DrbdResourceStatus;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Response for HA profile list
#[derive(Serialize)]
pub struct HaProfileListResponse {
    pub profiles: Vec<HaProfile>,
}

/// Response for HA profile creation
#[derive(Serialize)]
pub struct HaProfileCreateResponse {
    pub profile: HaProfile,
    pub config_path: String,
    pub message: String,
    /// Services that were disabled (if auto_disable_services was true)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub disabled_services: Vec<String>,
    /// Information about generated systemd units
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_units: Option<GeneratedUnits>,
    /// Data migration result (if migration was performed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_result: Option<MigrationResultInfo>,
    /// Nodes that were synced with HA configuration
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub synced_nodes: Vec<String>,
    /// Content of the generated promoter configuration file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promoter_config_content: Option<String>,
}

/// Summary of data migration result
#[derive(Serialize)]
pub struct MigrationResultInfo {
    pub bytes_transferred: u64,
    pub source_path: String,
    pub services_restarted: Vec<String>,
}

/// Response for HA profile detail
#[derive(Serialize)]
pub struct HaProfileDetailResponse {
    #[serde(flatten)]
    pub profile: HaProfile,
    // Override status with live status
    pub status: HaProfileStatus,
    /// Currently active node (from drbd-reactorctl)
    pub active_node: Option<String>,
    /// Detected mount point from mount unit (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_point: Option<String>,
    /// Detailed DRBD resource status
    pub drbd: Option<DrbdResourceStatus>,
    pub service_statuses: Vec<ServiceStatusInfo>,
    pub vip_active: Option<bool>,
    /// Configuration visibility info
    pub config: ConfigVisibility,
    /// Raw output from drbd-reactorctl status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reactor_status_raw: Option<String>,
}

/// Configuration visibility information
#[derive(Serialize)]
pub struct ConfigVisibility {
    /// Whether the promoter config file exists
    pub promoter_config_exists: bool,
    /// Path to the promoter config file
    pub promoter_config_path: String,
    /// Whether drbd-reactor service is running
    pub reactor_running: bool,
}

#[derive(Serialize)]
pub struct ServiceStatusInfo {
    pub name: String,
    pub active: bool,
    pub state: String,
    /// Whether the service is enabled (starts on boot)
    pub enabled: bool,
    /// When the service became active (Unix timestamp in seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_since: Option<i64>,
}

/// Query parameters for delete profile
#[derive(Debug, Deserialize)]
pub struct DeleteProfileQuery {
    /// Also delete the associated DRBD resource
    #[serde(default)]
    pub delete_resource: bool,
    /// Also delete the promoter configuration file from disk
    /// WARNING: This will remove the .toml file from /etc/drbd-reactor.d/
    /// The profile can still be re-imported from the file if it exists
    #[serde(default)]
    pub delete_config_file: bool,
}

/// Query parameters for reactor log retrieval
#[derive(Debug, Deserialize)]
pub struct ReactorLogsQuery {
    /// Number of lines to retrieve (default: 100, max: 1000)
    #[serde(default = "default_reactor_log_lines")]
    pub lines: u32,
    /// Filter logs since this time (e.g., "1h", "30m", "2024-01-15")
    pub since: Option<String>,
}

fn default_reactor_log_lines() -> u32 {
    100
}

/// Response for reactor logs
#[derive(Serialize)]
pub struct ReactorLogsResponse {
    pub service: String,
    pub lines: Vec<String>,
    pub total_lines: usize,
}

/// Query parameters for service listing
#[derive(Debug, Deserialize)]
pub struct ListServicesQuery {
    /// Include system services (default: false)
    #[serde(default)]
    pub include_system: bool,
}

/// Response for service list
#[derive(Serialize)]
pub struct ServiceListResponse {
    pub services: Vec<ServiceInfo>,
}

/// Response for service files list
#[derive(Serialize)]
pub struct ServiceFileListResponse {
    pub services: Vec<ServiceFileInfo>,
}

/// Request for reactor reload
#[derive(Debug, Deserialize)]
pub struct ReactorReloadRequest {
    /// Action: "reload" or "restart"
    #[serde(default = "default_reload_action")]
    pub action: String,
}

fn default_reload_action() -> String {
    "reload".to_string()
}

/// Response for reactor reload
#[derive(Serialize)]
pub struct ReactorReloadResponse {
    /// Local node result
    pub local: NodeReloadResult,
    /// Remote node results
    pub remote_nodes: Vec<NodeReloadResult>,
    /// Overall message
    pub message: String,
}

#[derive(Serialize)]
pub struct NodeReloadResult {
    pub hostname: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Request to import profiles
#[derive(Debug, Deserialize, ToSchema)]
pub struct ImportProfilesRequest {
    pub names: Vec<String>,
}

/// Response for import
#[derive(Serialize, ToSchema)]
pub struct ImportProfilesResponse {
    pub imported: Vec<String>,
    pub failed: Vec<String>,
}

/// Request for evicting an HA profile from a node
#[derive(Debug, Deserialize)]
pub struct EvictProfileRequest {
    /// Target node hostname or ID to evict from (optional, defaults to local node)
    pub node: Option<String>,
    /// Delay in seconds to wait for peer takeover (default: 20)
    #[serde(default = "default_evict_delay")]
    pub delay: u32,
    /// Keep the target masked after eviction (prevents automatic failback)
    #[serde(default)]
    pub keep_masked: bool,
    /// Force eviction even with warnings
    #[serde(default)]
    pub force: bool,
}

fn default_evict_delay() -> u32 {
    20
}

/// Response for evict operation
#[derive(Serialize)]
pub struct EvictProfileResponse {
    pub success: bool,
    pub node: String,
    pub profile: String,
    pub message: String,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

/// Request for adding VIP to a profile
#[derive(Debug, Deserialize)]
pub struct AddVipRequest {
    pub address: String,
    pub netmask: u8,
    pub interface: String,
}

/// Response for VIP operations
#[derive(Serialize)]
pub struct VipOperationResponse {
    pub message: String,
}

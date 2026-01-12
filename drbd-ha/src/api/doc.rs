//! OpenAPI documentation structure
//!
//! This module defines the `ApiDoc` struct which aggregates all API documentation.

use utoipa::OpenApi;

// Import models
use crate::models::{
    cluster::*,
    dashboard::*,
    drbd::{Path as DrbdPath, *},
    ha::{DataMigrationOptions, *},
    storage::*,
};

// Import handlers (aliased to avoid conflict with models modules)
use crate::api::{
    cluster::{self as cluster_api, HealthResponse, NodeStatusResponse},
    dashboard as dashboard_api,
    ha::{
        self as ha_api, AddVipRequest, ConfigVisibility, DeleteProfileQuery, EvictProfileRequest,
        EvictProfileResponse, HaProfileCreateResponse, HaProfileDetailResponse,
        HaProfileListResponse, ImportProfilesRequest, ImportProfilesResponse,
        ListServicesQuery, MigrationResultInfo, NodeConfigInfo, NodeReloadResult, ReactorLogsQuery,
        ReactorLogsResponse, ReactorReloadRequest, ReactorReloadResponse,
        ResourceAgentsByProvider, ResourceAgentDto, AgentSummary,
        ServiceFileListResponse, ServiceListResponse, ServiceStatusInfo, SyncTomlResponse, TomlContentResponse,
        TomlWithAgentsResponse, UpdateStartArrayRequest, UpdateStartArrayResponse,
        UpdateTomlRequest, VipOperationResponse,
    },
    metrics as metrics_api,
    resource::{self as resource_api, LogsQuery, LogsResponse, ResourceActionResponse, ResourceCreateResponse, ResourceListResponse},
    storage::{self as storage_api, ZpoolCheckResponse, ZpoolInfo},
};

#[derive(OpenApi)]
#[openapi(
    paths(
        // Cluster / Nodes
        cluster_api::health_check,
        cluster_api::list_nodes,
        cluster_api::add_node,
        cluster_api::get_node,
        cluster_api::update_node,
        cluster_api::delete_node,
        cluster_api::list_node_disks,
        cluster_api::list_available_disks,
        cluster_api::check_node_status,

        // Dashboard
        dashboard_api::get_summary,

        // Resources
        resource_api::list_resources,
        resource_api::get_resource,
        resource_api::create_resource,
        resource_api::delete_resource,
        resource_api::resource_action,
        resource_api::init_resource,
        resource_api::create_filesystem,
        resource_api::mount_resource,
        resource_api::umount_resource,
        resource_api::get_resource_logs,

        // Storage Pools
        storage_api::list_pools,
        storage_api::create_pool,
        storage_api::create_volume,
        storage_api::check_zpool,
        storage_api::check_zpool_on_node,

        // Metrics
        metrics_api::get_metrics,
        metrics_api::get_metrics_summary,
        metrics_api::health_with_metrics,

        // HA - Profiles
        ha_api::list_profiles,
        ha_api::get_profile,
        ha_api::get_profile_status,
        ha_api::create_profile,
        ha_api::delete_profile,
        ha_api::activate_profile,
        ha_api::deactivate_profile,
        ha_api::evict_profile,
        ha_api::enable_profile,
        ha_api::enable_profile_on_node,
        ha_api::disable_profile_on_node,

        // HA - VIP
        ha_api::add_vip,
        ha_api::remove_vip,

        // HA - TOML
        ha_api::get_profile_toml,
        ha_api::update_profile_toml,
        ha_api::sync_profile_toml,
        ha_api::parse_profile_toml,
        ha_api::update_start_array,

        // HA - Discovery and Import
        ha_api::list_unmanaged_profiles,
        ha_api::import_profiles,

        // HA - Resource Agents
        ha_api::list_resource_agents,
        ha_api::list_all_resource_agents,
        ha_api::get_resource_agent_metadata,

        // HA - Reactor
        ha_api::reactor_status,
        ha_api::reactor_logs,
        ha_api::reload_reactor,

        // HA - Services
        ha_api::list_services,
        ha_api::list_available_services,
    ),
    components(
        schemas(
            // Cluster
            Node, AddNodeRequest, NodeStatus, BlockDevice, LsblkOutput,
            HealthResponse, NodeStatusResponse,

            // Dashboard
            DashboardSummary, NodeStats, ResourceStats, StorageStats, HaServiceStats, ClusterHealth,

            // DRBD
            Resource, Device, Connection, BackingDevice, PeerDevice, DrbdPath,
            Role, DiskState, ConnectionState, ReplicationState,
            CreateResourceRequest, CreateFilesystemRequest, MountRequest,
            ResourceAction, ResourceActionRequest,
            ResourceListResponse, ResourceCreateResponse, ResourceActionResponse,
            LogsResponse, LogsQuery,

            // HA
            HaProfile, CreateHaProfileRequest,
            HaProfileStatus, HaType, PromoterSettings, GeneratedUnits,
            VipConfig, DataMigrationOptions, MountStrategy,
            ServiceOverride, PromoterConfig, PromoterResources,
            ImportProfilesRequest, ImportProfilesResponse,
            HaProfileListResponse, HaProfileCreateResponse, HaProfileDetailResponse,
            MigrationResultInfo, NodeConfigInfo, ConfigVisibility, ServiceStatusInfo,
            DeleteProfileQuery, ReactorLogsQuery, ReactorLogsResponse,
            ListServicesQuery, ServiceListResponse, ServiceFileListResponse,
            ReactorReloadRequest, ReactorReloadResponse, NodeReloadResult,
            EvictProfileRequest, EvictProfileResponse,
            AddVipRequest, VipOperationResponse,
            OcfAgentConfig,
            UpdateTomlRequest, TomlContentResponse,
            SyncTomlResponse, UpdateStartArrayRequest, UpdateStartArrayResponse,
            TomlWithAgentsResponse,
            ResourceAgentsByProvider,
            ResourceAgentDto, AgentSummary,

            // Storage
            StoragePool, Volume, CreateStoragePoolRequest, CreateStoragePoolResponse, ListStoragePoolResponse,
            CreateVolumeRequest, CreateVolumeResponse,
            ZpoolCheckResponse, ZpoolInfo,
        )
    ),
    tags(
        (name = "cluster", description = "Cluster node management"),
        (name = "dashboard", description = "System overview"),
        (name = "resources", description = "DRBD resource management"),
        (name = "storage", description = "LVM/ZFS storage pool management"),
        (name = "metrics", description = "Prometheus metrics"),
        (name = "ha", description = "High availability profile management"),
    )
)]
pub struct ApiDoc;
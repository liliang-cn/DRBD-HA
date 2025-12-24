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
    ha::{self as ha_api, ImportProfilesRequest, ImportProfilesResponse}, // Import DTOs here
    storage as storage_api,
};

#[derive(OpenApi)]
#[openapi(
    paths(
        // Cluster / Nodes
        cluster_api::health_check,
        cluster_api::list_nodes,
        cluster_api::add_node,
        cluster_api::get_node,
        cluster_api::delete_node,
        cluster_api::list_node_disks,
        cluster_api::list_available_disks,
        cluster_api::check_node_status,

        // Dashboard
        dashboard_api::get_summary,

        // Storage Pools
        storage_api::list_pools,
        storage_api::create_pool,
        storage_api::create_volume,

        // HA
        ha_api::list_unmanaged_profiles,
        ha_api::import_profiles,
    ),
    components(
        schemas(
            // Cluster
            Node, AddNodeRequest, NodeStatus, BlockDevice, LsblkOutput, HealthResponse, NodeStatusResponse,

            // Dashboard
            DashboardSummary, NodeStats, ResourceStats, StorageStats, HaServiceStats, ClusterHealth,

            // DRBD
            Resource, Device, Connection, BackingDevice, PeerDevice, DrbdPath,
            Role, DiskState, ConnectionState, ReplicationState,
            CreateResourceRequest, CreateFilesystemRequest, MountRequest,
            ResourceAction, ResourceActionRequest,

            // HA
            HaProfile, CreateHaProfileRequest,
            HaProfileStatus, HaType, PromoterSettings, GeneratedUnits,
            VipConfig, DataMigrationOptions,
            ServiceOverride, PromoterConfig, PromoterResources,
            ImportProfilesRequest, ImportProfilesResponse,

            // Storage
            StoragePool, Volume, CreateStoragePoolRequest, CreateStoragePoolResponse, ListStoragePoolResponse,
            CreateVolumeRequest, CreateVolumeResponse,
        )
    ),
    tags(
        (name = "cluster", description = "Cluster node management"),
        (name = "dashboard", description = "System overview"),
        (name = "storage", description = "LVM storage pool management"),
    )
)]
pub struct ApiDoc;
//! Dashboard data models

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ClusterHealth {
    Healthy,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardSummary {
    pub health: ClusterHealth,
    pub nodes: NodeStats,
    pub resources: ResourceStats,
    pub storage: StorageStats,
    pub ha_services: HaServiceStats,
    pub ha_service_details: Vec<HaServiceDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HaServiceDetail {
    pub name: String,
    pub active_node: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeStats {
    pub total: usize,
    pub online: usize,
    pub offline: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResourceStats {
    pub total: usize,
    pub healthy: usize,
    pub degraded: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StorageStats {
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub pool_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HaServiceStats {
    pub total: usize,
    pub active: usize,
    pub standby: usize,
    pub stopped: usize,
    pub error: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_health_deserialization() {
        let json = r#""healthy""#;
        let health: ClusterHealth = serde_json::from_str(json).unwrap();
        assert_eq!(health, ClusterHealth::Healthy);

        let json = r#""warning""#;
        let health: ClusterHealth = serde_json::from_str(json).unwrap();
        assert_eq!(health, ClusterHealth::Warning);

        let json = r#""critical""#;
        let health: ClusterHealth = serde_json::from_str(json).unwrap();
        assert_eq!(health, ClusterHealth::Critical);
    }
}

//! Dashboard API handlers

use axum::{extract::State, Json};
use std::sync::Arc;

use crate::core::drbd_cmd::DrbdCmd;
use crate::core::{run_shell_command, ReactorDiscovery};
use crate::error::AppResult;
use crate::models::{
    ClusterHealth, DashboardSummary, HaProfile, HaProfileStatus, HaServiceDetail, HaServiceStats,
    NodeStats, NodeStatus, ResourceStats, StorageStats,
};
use crate::state::AppState;
use drbd_reactor_utils::DrbdReactorClient;

/// GET /api/v1/dashboard/summary
#[utoipa::path(
    get,
    path = "/api/v1/dashboard/summary",
    tag = "dashboard",
    responses(
        (status = 200, description = "Dashboard summary", body = DashboardSummary)
    )
)]
pub async fn get_summary(State(state): State<Arc<AppState>>) -> AppResult<Json<DashboardSummary>> {
    // 1. Node Stats
    let nodes = state.node_store.get_all()?;
    let total_nodes = nodes.len();
    let online_nodes = nodes
        .iter()
        .filter(|n| n.status == NodeStatus::Online)
        .count();
    let offline_nodes = total_nodes - online_nodes;

    let node_stats = NodeStats {
        total: total_nodes,
        online: online_nodes,
        offline: offline_nodes,
    };

    // 2. Storage Stats - scan from LVM directly
    let vg_infos = crate::core::list_vg_info().await.unwrap_or_default();
    let total_bytes: u64 = vg_infos.iter().map(|p| p.size).sum();
    let free_bytes: u64 = vg_infos.iter().map(|p| p.free).sum();

    let storage_stats = StorageStats {
        total_bytes,
        free_bytes,
        pool_count: vg_infos.len(),
    };

    // 3. HA Service Stats
    let profiles = ReactorDiscovery::scan_profiles().await?;
    let active = profiles
        .iter()
        .filter(|p| p.status == HaProfileStatus::Active)
        .count();
    let standby = profiles
        .iter()
        .filter(|p| p.status == HaProfileStatus::Standby)
        .count();
    let stopped = profiles
        .iter()
        .filter(|p| p.status == HaProfileStatus::Stopped)
        .count();
    let error = profiles
        .iter()
        .filter(|p| p.status == HaProfileStatus::Error)
        .count();

    let ha_service_stats = HaServiceStats {
        total: profiles.len(),
        active,
        standby,
        stopped,
        error,
    };

    // 4. Resource Stats (DRBD) - Check live status via drbdadm status
    // This gives a quick overview of all resources
    // Output format:
    // resource-name role:Primary disk:UpToDate ...
    let drbd_output = run_shell_command(&DrbdCmd::status_all_cmd(), "Get all DRBD status").await;

    let (total_res, healthy_res, degraded_res) = if let Ok(output) = drbd_output {
        if output.success() {
            parse_drbd_summary(&output.stdout)
        } else {
            (0, 0, 0)
        }
    } else {
        (0, 0, 0)
    };

    let resource_stats = ResourceStats {
        total: total_res,
        healthy: healthy_res,
        degraded: degraded_res,
    };

    // 4.5 Get HA Service Details from drbd-reactorctl
    let ha_service_details = get_ha_service_details(&profiles).await.unwrap_or_default();

    // 5. Determine Cluster Health
    let health = if offline_nodes > 0 || error > 0 || degraded_res > 0 {
        if offline_nodes > total_nodes / 2 {
            // Simple quorum check approximation
            ClusterHealth::Critical
        } else {
            ClusterHealth::Warning
        }
    } else {
        ClusterHealth::Healthy
    };

    Ok(Json(DashboardSummary {
        health,
        nodes: node_stats,
        resources: resource_stats,
        storage: storage_stats,
        ha_services: ha_service_stats,
        ha_service_details,
    }))
}

fn parse_drbd_summary(output: &str) -> (usize, usize, usize) {
    let mut total = 0;

    let mut healthy = 0;

    let mut degraded = 0;

    let lines: Vec<&str> = output.lines().collect();

    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        if !line.starts_with(' ') && !line.is_empty() {
            // Resource header line

            total += 1;

            let mut is_resource_healthy = true;

            // Check local disk status (line i + 1)

            if i + 1 < lines.len() {
                let local_disk_line = lines[i + 1];

                if local_disk_line.contains("disk:Inconsistent")
                    || local_disk_line.contains("disk:Failed")
                    || local_disk_line.contains("disk:Diskless")
                {
                    is_resource_healthy = false;
                }
            }

            // Check peer connections and disk status

            let mut j = i + 2; // Start checking from peer lines

            while j < lines.len() {
                let sub_line = lines[j];

                if !sub_line.starts_with(' ') {
                    break; // End of this resource block
                }

                // Check connection state

                if sub_line.contains("connection:Connecting")
                    || sub_line.contains("connection:StandAlone")
                {
                    is_resource_healthy = false;
                }

                // Check peer-disk state

                if sub_line.contains("peer-disk:Inconsistent")
                    || sub_line.contains("peer-disk:Failed")
                    || sub_line.contains("peer-disk:Diskless")
                {
                    is_resource_healthy = false;
                }

                j += 1;
            }

            if is_resource_healthy {
                healthy += 1;
            } else {
                degraded += 1;
            }
        }

        i += 1;
    }

    (total, healthy, degraded)
}

// Parse drbd-reactorctl status --json to extract HA service details
async fn get_ha_service_details(profiles: &[HaProfile]) -> AppResult<Vec<HaServiceDetail>> {
    let (statuses, _) = match DrbdReactorClient::status(None, None).await {
        Ok(res) => res,
        Err(_) => return Ok(Vec::new()),
    };

    let mut services = Vec::new();
    let profile_map: std::collections::HashMap<String, &HaProfile> =
        profiles.iter().map(|p| (p.name.clone(), p)).collect();

    for status in statuses {
        if let Some(profile) = profile_map.get(&status.name) {
            let service_type = serde_json::to_string(&profile.ha_type)
                .unwrap_or_else(|_| "generic".to_string())
                .trim_matches('"')
                .to_string();

            services.push(HaServiceDetail {
                name: status.name,
                active_node: status.active_node.clone(),
                status: if status.is_active {
                    "active"
                } else {
                    "standby"
                }
                .to_string(),
                service_type,
                vip: profile.vip.as_ref().map(|v| v.address.clone()),
                export_path: Some(profile.mount_point.clone()),
                nodes: vec![], // Placeholder
            });
        }
    }

    Ok(services)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_drbd_summary_empty() {
        let output = "";
        let (total, healthy, degraded) = parse_drbd_summary(output);
        assert_eq!(total, 0);
        assert_eq!(healthy, 0);
        assert_eq!(degraded, 0);
    }

    #[test]
    fn test_parse_drbd_summary_healthy() {
        let output = r#"
r0 role:Primary
  disk:UpToDate
  peer-0 role:Secondary
    connection:Connected peer-disk:UpToDate
r1 role:Secondary
  disk:UpToDate
  peer-0 role:Primary
    connection:Connected peer-disk:UpToDate
"#;
        let (total, healthy, degraded) = parse_drbd_summary(output);
        assert_eq!(total, 2);
        assert_eq!(healthy, 2);
        assert_eq!(degraded, 0);
    }

    #[test]
    fn test_parse_drbd_summary_degraded() {
        let output = r#"
r0 role:Primary
  disk:UpToDate
  peer-0 role:Secondary
    connection:Connecting peer-disk:UpToDate
r1 role:Secondary
  disk:Inconsistent
  peer-0 role:Primary
    connection:Connected peer-disk:UpToDate
r2 role:Primary
  disk:UpToDate
  peer-0 role:Secondary
    connection:Connected peer-disk:Inconsistent
r3 role:Secondary
  disk:Diskless
  peer-0 role:Primary
    connection:Connected peer-disk:UpToDate
"#;
        let (total, healthy, degraded) = parse_drbd_summary(output);
        assert_eq!(total, 4);
        assert_eq!(healthy, 0); // All have some form of degradation
        assert_eq!(degraded, 4);
    }

    #[test]
    fn test_parse_drbd_summary_mixed() {
        let output = r#"
r0 role:Primary
  disk:UpToDate
  peer-0 role:Secondary
    connection:Connected peer-disk:UpToDate
r1 role:Secondary
  disk:UpToDate
  peer-0 role:Primary
    connection:Connecting peer-disk:UpToDate
r2 role:Secondary
  disk:Inconsistent
  peer-0 role:Primary
    connection:Connected peer-disk:UpToDate
"#;
        let (total, healthy, degraded) = parse_drbd_summary(output);
        assert_eq!(total, 3);
        assert_eq!(healthy, 1); // Only r0 is healthy
        assert_eq!(degraded, 2);
    }
}

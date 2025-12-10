//! Dashboard API handlers

use axum::{extract::State, Json};
use std::sync::Arc;

use crate::core::run_shell_command;
use crate::error::AppResult;
use crate::models::{
    ClusterHealth, DashboardSummary, HaProfileStatus, HaServiceDetail, HaServiceStats, NodeStats,
    NodeStatus, ResourceStats, StorageStats,
};
use crate::state::AppState;

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
    // ...
    let nodes = state.db.get_all_nodes()?;
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

    // 2. Storage Stats
    let pools = state.db.get_all_storage_pools()?;
    let total_bytes: u64 = pools.iter().map(|p| p.total_size).sum();
    let free_bytes: u64 = pools.iter().map(|p| p.free_size).sum();

    let storage_stats = StorageStats {
        total_bytes,
        free_bytes,
        pool_count: pools.len(),
    };

    // 3. HA Service Stats (from DB, might be stale status but acceptable for summary,
    // real-time updates come via SSE)
    // Ideally we should have a background task updating DB statuses.
    // For now, we count based on what's in DB.
    let profiles = state.db.get_all_ha_profiles()?;
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
    let drbd_output = run_shell_command("drbdadm status", "Get all DRBD status").await;

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
    let db_profiles = state.db.get_all_ha_profiles()?;
    let db_service_names: std::collections::HashSet<String> =
        db_profiles.iter().map(|p| p.name.clone()).collect();
    let ha_service_details = get_ha_service_details(&db_service_names)
        .await
        .unwrap_or_default();

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

// Parse drbd-reactorctl status to extract HA service details
async fn get_ha_service_details(
    db_service_names: &std::collections::HashSet<String>,
) -> AppResult<Vec<HaServiceDetail>> {
    let output = run_shell_command("drbd-reactorctl status", "Get drbd-reactor status").await;

    match output {
        Ok(result) if result.success() => {
            Ok(parse_ha_service_details(&result.stdout, db_service_names))
        }
        _ => {
            // If command fails, return empty list
            Ok(Vec::new())
        }
    }
}

fn parse_ha_service_details(
    output: &str,
    db_service_names: &std::collections::HashSet<String>,
) -> Vec<HaServiceDetail> {
    let mut services = Vec::new();
    let mut current_service: Option<String> = None;
    let mut current_active_node: Option<String> = None;

    for line in output.lines() {
        // Match profile file paths: "/etc/drbd-reactor.d/mongodb-ha.toml:"
        if line.contains("/etc/drbd-reactor.d/") && line.ends_with(':') {
            // If we have a previous service, save it (only if in database)
            if let Some(service_name) = current_service.take() {
                if db_service_names.contains(&service_name) {
                    let has_active_node = current_active_node.is_some();
                    services.push(HaServiceDetail {
                        name: service_name,
                        active_node: current_active_node.take(),
                        status: if has_active_node { "active" } else { "standby" }.to_string(),
                    });
                } else {
                    current_active_node.take(); // Clear it even if not saving
                }
            }

            // Extract service name from path
            if let Some(filename) = line.rsplit('/').next() {
                if let Some(name) = filename.strip_suffix(".toml:") {
                    current_service = Some(name.to_string());
                }
            }
        }

        // Match lines like "Promoter: Currently active on node 'gui02'"
        if line.contains("Promoter: Currently active on node") {
            if let Some(start) = line.find('\'') {
                if let Some(end) = line.rfind('\'') {
                    if end > start {
                        let node_name = &line[start + 1..end];
                        current_active_node = Some(node_name.to_string());
                    }
                }
            }
        }
    }

    // Don't forget the last service (only if in database)
    if let Some(service_name) = current_service {
        if db_service_names.contains(&service_name) {
            let has_active_node = current_active_node.is_some();
            services.push(HaServiceDetail {
                name: service_name,
                active_node: current_active_node,
                status: if has_active_node { "active" } else { "standby" }.to_string(),
            });
        }
    }

    services
}
#[cfg(test)]
mod ha_service_tests {
    use super::*;

    #[test]
    fn test_parse_ha_service_details() {
        let output = r#"/etc/drbd-reactor.d/linstor_controller.toml:
Promoter: Currently active on node 'gui02'
/etc/drbd-reactor.d/mongodb-ha.toml:
Promoter: Currently active on node 'gui02'
/etc/drbd-reactor.d/mysql-ha.toml:
Promoter: Currently active on node 'gui03'
/etc/drbd-reactor.d/redis-ha.toml:
Promoter: Currently active on node 'gui03'
/etc/drbd-reactor.d/prometheus.toml:
Prometheus: listening on 0.0.0.0:9942
"#;
        let mut db_services = std::collections::HashSet::new();
        db_services.insert("linstor_controller".to_string());
        db_services.insert("mongodb-ha".to_string());
        db_services.insert("mysql-ha".to_string());
        db_services.insert("redis-ha".to_string());

        let details = parse_ha_service_details(output, &db_services);
        assert_eq!(details.len(), 4); // prometheus should be filtered out
        assert_eq!(details[0].name, "linstor_controller");
        assert_eq!(details[0].active_node, Some("gui02".to_string()));
        assert_eq!(details[3].name, "redis-ha");
        assert_eq!(details[3].active_node, Some("gui03".to_string()));
    }
}

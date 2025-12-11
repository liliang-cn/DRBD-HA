//! Dashboard API handlers

use axum::{extract::State, Json};
use std::collections::HashMap;
use std::sync::Arc;

use crate::core::drbd_cmd::{parse_drbd_status, ResourceStatus};
use crate::core::run_shell_command;
use crate::error::AppResult;
use crate::models::{
    ClusterHealth, DashboardSummary, HaProfile, HaProfileStatus, HaServiceDetail, HaServiceStats,
    HaType, NodeStats, NodeStatus, ResourceStats, StorageStats,
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
    // 1. Node Stats
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

    // 3. HA Service Stats (from DB)
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

    // 4. Resource Stats (DRBD) - Check live status via drbdadm status --json
    let drbd_output = run_shell_command("drbdadm status --json", "Get all DRBD status").await;

    let resource_statuses = if let Ok(output) = drbd_output {
        if output.success() {
            parse_drbd_status(&output.stdout).unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let resource_stats = calculate_resource_stats(&resource_statuses);

    // 4.5 Get HA Service Details (Enriched)
    // We need the local hostname to identify "this node" in the list of nodes
    let hostname_output = run_shell_command("uname -n", "Get hostname").await;
    let hostname = if let Ok(output) = hostname_output {
        output.stdout.trim().to_string()
    } else {
        "localhost".to_string()
    };

    let ha_service_details =
        get_ha_service_details(&profiles, &resource_statuses, &hostname).await?;

    // 5. Determine Cluster Health
    let health = if offline_nodes > 0 || error > 0 || resource_stats.degraded > 0 {
        if offline_nodes > total_nodes / 2 {
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

fn calculate_resource_stats(statuses: &[ResourceStatus]) -> ResourceStats {
    let total = statuses.len();
    let mut healthy = 0;
    let mut degraded = 0;

    for res in statuses {
        let mut is_healthy = true;

        // Check local disk
        for dev in &res.devices {
            if dev.disk_state != "UpToDate" && dev.disk_state != "Diskless" {
                // Diskless is okay if it's intentional, but usually implies some degradation if not primary?
                // Actually, if it's UpToDate, it's fine.
                // If it is inconsistent or failed, it's bad.
                if dev.disk_state == "Inconsistent" || dev.disk_state == "Failed" {
                    is_healthy = false;
                }
            }
        }

        // Check connections
        for conn in &res.connections {
            if conn.connection_state != "Connected" {
                is_healthy = false;
            }
            for peer_dev in &conn.peer_devices {
                if peer_dev.peer_disk_state == "Inconsistent"
                    || peer_dev.peer_disk_state == "Failed"
                {
                    is_healthy = false;
                }
            }
        }

        if is_healthy {
            healthy += 1;
        } else {
            degraded += 1;
        }
    }

    ResourceStats {
        total,
        healthy,
        degraded,
    }
}

// Parse drbd-reactorctl status to extract HA service details and combine with profile info
async fn get_ha_service_details(
    profiles: &[HaProfile],
    resource_statuses: &[ResourceStatus],
    hostname: &str,
) -> AppResult<Vec<HaServiceDetail>> {
    // Get active nodes from reactor
    let reactor_output =
        run_shell_command("drbd-reactorctl status", "Get drbd-reactor status").await;

    let active_node_map = match reactor_output {
        Ok(result) if result.success() => parse_reactor_active_nodes(&result.stdout, hostname),
        _ => HashMap::new(),
    };

    // Build resource map for quick lookup
    let resource_map: HashMap<&str, &ResourceStatus> = resource_statuses
        .iter()
        .map(|r| (r.name.as_str(), r))
        .collect();

    let mut details = Vec::new();

    for profile in profiles {
        let active_node = active_node_map.get(&profile.name).cloned();
        let status = if active_node.is_some() {
            "active"
        } else {
            "standby" // simplistic status, ideally check if it's really standby or stopped
        }
        .to_string();

        // Nodes involved
        let mut nodes = Vec::new();
        if let Some(res_status) = resource_map.get(profile.resource_name.as_str()) {
            // Add local node
            nodes.push(hostname.to_string());
            // Add peer nodes
            for conn in &res_status.connections {
                nodes.push(conn.name.clone());
            }
        }
        nodes.sort();
        nodes.dedup();

        // VIP info
        let vip = profile.vip.as_ref().map(|v| v.cidr());

        // Service Type & Export Path
        let (service_type, export_path) = match profile.ha_type {
            HaType::Nfs => (
                "NFS".to_string(),
                profile.nfs.as_ref().map(|n| n.export_path.clone()),
            ),
            HaType::Iscsi => ("iSCSI".to_string(), None),
            HaType::NvmeOf => ("NVMe-oF".to_string(), None),
            HaType::Generic => ("Generic".to_string(), None),
        };

        details.push(HaServiceDetail {
            name: profile.name.clone(),
            active_node,
            status,
            service_type,
            vip,
            export_path,
            nodes,
        });
    }

    Ok(details)
}

fn parse_reactor_active_nodes(output: &str, hostname: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut current_service: Option<String> = None;

    for line in output.lines() {
        // Match profile file paths: "/etc/drbd-reactor.d/mongodb-ha.toml:"
        if line.contains("/etc/drbd-reactor.d/") && line.ends_with(':') {
            // Extract service name from path
            if let Some(filename) = line.rsplit('/').next() {
                if let Some(name) = filename.strip_suffix(".toml:") {
                    current_service = Some(name.to_string());
                }
            }
        }

        // Match lines like "Promoter: Currently active on node 'gui02'"
        // or "Promoter: Currently active on this node"
        if line.contains("Promoter: Currently active on") {
            if let Some(service_name) = &current_service {
                if line.contains("on this node") {
                    map.insert(service_name.clone(), hostname.to_string());
                } else {
                    // Check for single quotes first (standard output), then backticks (fallback)
                    let node_name = if let Some(start) = line.find('\'') {
                        if let Some(end) = line.rfind('\'') {
                            if end > start {
                                Some(&line[start + 1..end])
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else if let Some(start) = line.find('`') {
                        if let Some(end) = line.rfind('`') {
                            if end > start {
                                Some(&line[start + 1..end])
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(name) = node_name {
                        map.insert(service_name.clone(), name.to_string());
                    }
                }
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reactor_active_nodes() {
        let output = r#"/etc/drbd-reactor.d/linstor_controller.toml:
Promoter: Currently active on node 'gui02'
/etc/drbd-reactor.d/mongodb-ha.toml:
Promoter: Currently active on node 'gui02'
/etc/drbd-reactor.d/mysql-ha.toml:
Promoter: Currently active on node 'gui03'
/etc/drbd-reactor.d/redis-ha.toml:
Promoter: Currently active on this node
"#;
        let hostname = "gui03";
        let map = parse_reactor_active_nodes(output, hostname);
        assert_eq!(map.len(), 4);
        assert_eq!(map.get("linstor_controller"), Some(&"gui02".to_string()));
        assert_eq!(map.get("redis-ha"), Some(&"gui03".to_string()));
    }
}

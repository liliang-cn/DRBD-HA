use crate::models::{BuiltinPluginStatus, ReactorProfileStatus, ReactorServiceDetail, ReactorServiceStatus};

/// Detect if a drbd-reactor config file is a built-in plugin (e.g., prometheus, events)
/// rather than a promoter-based HA profile
pub fn is_builtin_plugin(toml_content: &str, profile_name: &str) -> bool {
    // Built-in plugins don't have [promoter] section
    if toml_content.contains("[promoter]") {
        return false;
    }

    // Check for known built-in plugin sections
    if toml_content.contains("[[prometheus]]")
        || toml_content.contains("[[events]]")
        || toml_content.contains("[[grafana]]")
    {
        return true;
    }

    // Check by known profile names
    matches!(profile_name, "prometheus" | "events" | "grafana")
}

/// Parse status for built-in plugins from drbd-reactorctl output
/// Expected format:
///   /etc/drbd-reactor.d/prometheus.toml:
///   Prometheus: listening on 0.0.0.0:9942
///
/// Returns Some(status) if this is a built-in plugin status line, None otherwise
pub fn parse_builtin_plugin_status(line: &str, profile_name: &str) -> Option<BuiltinPluginStatus> {
    let trimmed = line.trim();

    // Built-in plugin status format: "PluginType: listening on address"
    // e.g., "Prometheus: listening on 0.0.0.0:9942"
    //       "Events: listening on /run/drbd-reactor/events.sock"

    if !trimmed.contains(": listening on") {
        return None;
    }

    // Extract plugin type and address
    let parts: Vec<&str> = trimmed.splitn(2, ':').collect();
    if parts.len() != 2 {
        return None;
    }

    let plugin_type = parts[0].trim();
    let status_part = parts[1].trim();

    // Check if this matches the profile we're looking for
    // Map plugin type to profile name (case-insensitive)
    let is_match = match plugin_type.to_lowercase().as_str() {
        "prometheus" => profile_name.eq_ignore_ascii_case("prometheus"),
        "events" => profile_name.eq_ignore_ascii_case("events"),
        "grafana" => profile_name.eq_ignore_ascii_case("grafana"),
        _ => false,
    };

    if !is_match {
        return None;
    }

    // Parse "listening on address"
    if !status_part.starts_with("listening on") {
        return None;
    }

    let address = status_part["listening on".len()..].trim().to_string();

    Some(BuiltinPluginStatus {
        name: profile_name.to_string(),
        plugin_type: plugin_type.to_string(),
        is_listening: true,
        address: Some(address),
    })
}

pub fn parse_reactor_status(output: &str, profile_name: Option<&str>) -> Vec<ReactorProfileStatus> {
    let mut statuses = Vec::new();
    // Simplified parsing logic based on observation
    // Expected format for `drbd-reactorctl status`:
    // Promoter profiles:
    //   /etc/drbd-reactor.d/profile.toml:
    //   Promoter: Currently active on node 'node1'
    // Built-in plugins:
    //   /etc/drbd-reactor.d/prometheus.toml:
    //   Prometheus: listening on 0.0.0.0:9942

    let mut current_profile: Option<String> = None;
    let mut current_active_node: Option<String> = None;
    let mut builtin_plugin_active = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.contains("/etc/drbd-reactor.d/") && trimmed.ends_with(':') {
            // Save previous
            if let Some(name) = current_profile.take() {
                statuses.push(ReactorProfileStatus {
                    name,
                    // For builtin plugins, use local hostname as active node
                    active_node: if builtin_plugin_active {
                        Some(gethostname::gethostname().to_string_lossy().to_string())
                    } else {
                        current_active_node.clone()
                    },
                    is_active: builtin_plugin_active || current_active_node.is_some(),
                });
                current_active_node = None;
                builtin_plugin_active = false;
            }

            // New profile
            if let Some(filename) = trimmed.rsplit('/').next() {
                if let Some(name) = filename.strip_suffix(".toml:") {
                    current_profile = Some(name.to_string());
                }
            }
        }

        // Check for built-in plugin status (e.g., "Prometheus: listening on 0.0.0.0:9942")
        if trimmed.contains(": listening on") {
            builtin_plugin_active = true;
        }
        // Promoter status for regular profiles
        else if trimmed.contains("Promoter: Currently active on node") {
            if let Some(start) = trimmed.find('\x27') {
                if let Some(end) = trimmed.rfind('\x27') {
                    if end > start {
                        let node_name = &trimmed[start + 1..end];
                        current_active_node = Some(node_name.to_string());
                    }
                }
            }
        } else if trimmed.contains("Promoter: Currently active on this node") {
            current_active_node = Some(gethostname::gethostname().to_string_lossy().to_string());
        }
    }

    if let Some(name) = current_profile {
        statuses.push(ReactorProfileStatus {
            name,
            active_node: if builtin_plugin_active {
                Some(gethostname::gethostname().to_string_lossy().to_string())
            } else {
                current_active_node.clone()
            },
            is_active: builtin_plugin_active || current_active_node.is_some(),
        });
    }

    if let Some(filter) = profile_name {
        statuses.into_iter().filter(|s| s.name == filter).collect()
    } else {
        statuses
    }
}

pub fn parse_service_details(output: &str) -> Vec<ReactorServiceDetail> {
    let statuses = parse_reactor_status(output, None);
    statuses
        .into_iter()
        .map(|s| ReactorServiceDetail {
            name: s.name,
            active_node: s.active_node.clone(),
            status: if s.is_active {
                "active".to_string()
            } else {
                "standby".to_string()
            },
        })
        .collect()
}

pub fn parse_reactor_services(output: &str) -> Vec<ReactorServiceStatus> {
    let mut services = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        // Skip empty lines and header lines
        if trimmed.is_empty() || trimmed.contains("Promoter:") || trimmed.ends_with(".toml:") {
            continue;
        }

        let is_running = trimmed.starts_with('●') || trimmed.starts_with("●");
        let is_dead = trimmed.starts_with('○') || trimmed.starts_with("○");
        let is_failed = trimmed.starts_with('×') || trimmed.starts_with("×");

        if !is_running && !is_dead && !is_failed {
            continue;
        }

        let is_active = is_running;

        let without_symbol = if is_running {
            trimmed.strip_prefix('●').or(trimmed.strip_prefix("●"))
        } else if is_dead {
            trimmed.strip_prefix('○').or(trimmed.strip_prefix("○"))
        } else {
            trimmed.strip_prefix('×').or(trimmed.strip_prefix("×"))
        }
        .unwrap_or(trimmed);

        let name = without_symbol
            .trim_start_matches([' ', '├', '└', '─', '│'])
            .split_whitespace()
            .next()
            .unwrap_or("");

        if name.is_empty()
            || name.starts_with("drbd-services@")
            || name.starts_with("drbd-promote@")
        {
            continue;
        }

        let clean_name = name.replace("\\x2d", "-");

        let state_str = if is_active {
            "active".to_string()
        } else if is_failed {
            "failed".to_string()
        } else {
            "inactive".to_string()
        };

        services.push(ReactorServiceStatus {
            name: clean_name,
            active: is_active,
            state: state_str,
        });
    }
    services
}

#[cfg(test)]
mod tests {
    use super::*;
    use gethostname::gethostname;

    #[test]
    fn test_parse_reactor_status_empty() {
        let output = "";
        let statuses = parse_reactor_status(output, None);
        assert!(statuses.is_empty());
    }

    #[test]
    fn test_parse_reactor_status_single_active_local() {
        let output = r#"/etc/drbd-reactor.d/test-profile.toml:
Promoter: Currently active on this node"#;
        let statuses = parse_reactor_status(output, None);
        assert_eq!(statuses.len(), 1);
        let status = &statuses[0];
        assert_eq!(status.name, "test-profile");
        assert!(status.is_active);
        assert_eq!(
            status.active_node,
            Some(gethostname().to_string_lossy().to_string())
        );
    }

    #[test]
    fn test_parse_reactor_status_single_active_remote() {
        let output = r#"/etc/drbd-reactor.d/test-profile.toml:
Promoter: Currently active on node 'remote-node-1'"#;
        let statuses = parse_reactor_status(output, None);
        assert_eq!(statuses.len(), 1);
        let status = &statuses[0];
        assert_eq!(status.name, "test-profile");
        assert!(status.is_active);
        assert_eq!(status.active_node, Some("remote-node-1".to_string()));
    }

    #[test]
    fn test_parse_reactor_status_single_standby() {
        let output = r#"/etc/drbd-reactor.d/test-profile.toml:
Promoter: currently standby"#;
        let statuses = parse_reactor_status(output, None);
        assert_eq!(statuses.len(), 1);
        let status = &statuses[0];
        assert_eq!(status.name, "test-profile");
        assert!(!status.is_active);
        assert_eq!(status.active_node, None);
    }

    #[test]
    fn test_parse_reactor_status_multiple_profiles() {
        let output = r#"/etc/drbd-reactor.d/profile-one.toml:
Promoter: Currently active on node 'node-a'
/etc/drbd-reactor.d/profile-two.toml:
Promoter: currently standby
/etc/drbd-reactor.d/profile-three.toml:
Promoter: Currently active on this node"#;
        let statuses = parse_reactor_status(output, None);
        assert_eq!(statuses.len(), 3);

        let status1 = &statuses[0];
        assert_eq!(status1.name, "profile-one");
        assert!(status1.is_active);
        assert_eq!(status1.active_node, Some("node-a".to_string()));

        let status2 = &statuses[1];
        assert_eq!(status2.name, "profile-two");
        assert!(!status2.is_active);
        assert_eq!(status2.active_node, None);

        let status3 = &statuses[2];
        assert_eq!(status3.name, "profile-three");
        assert!(status3.is_active);
        assert_eq!(
            status3.active_node,
            Some(gethostname().to_string_lossy().to_string())
        );
    }

    #[test]
    fn test_parse_reactor_status_filtered() {
        let output = r#"/etc/drbd-reactor.d/profile-one.toml:
Promoter: Currently active on node 'node-a'
/etc/drbd-reactor.d/profile-two.toml:
Promoter: currently standby"#;
        let statuses = parse_reactor_status(output, Some("profile-one"));
        assert_eq!(statuses.len(), 1);
        let status = &statuses[0];
        assert_eq!(status.name, "profile-one");
        assert!(status.is_active);
        assert_eq!(status.active_node, Some("node-a".to_string()));
    }

    #[test]
    fn test_parse_service_details() {
        let output = r#"/etc/drbd-reactor.d/profile-one.toml:
Promoter: Currently active on node 'node-a'
/etc/drbd-reactor.d/profile-two.toml:
Promoter: currently standby"#;
        let details = parse_service_details(output);
        assert_eq!(details.len(), 2);
        assert_eq!(details[0].name, "profile-one");
        assert_eq!(details[0].status, "active");
        assert_eq!(details[0].active_node, Some("node-a".to_string()));
        assert_eq!(details[1].name, "profile-two");
        assert_eq!(details[1].status, "standby");
        assert_eq!(details[1].active_node, None);
    }

    #[test]
    fn test_parse_reactor_services() {
        let output = r#"/etc/drbd-reactor.d/mysql_ha.toml:
Promoter: Currently active on this node
● drbd-services@mysql_data.target
● ├─ drbd-promote@mysql_data.service
● ├─ var-lib-mysql.mount 
● ├─ ocf.rs@mysql_data_vip_mysql_data.service 
● └─ mysql.service 
○ └─ inactive.service
× └─ failed.service"#;
        let services = parse_reactor_services(output);
        // Corrected expected length to 5
        assert_eq!(services.len(), 5);

        let mount = services
            .iter()
            .find(|s| s.name == "var-lib-mysql.mount")
            .unwrap();
        assert!(mount.active);
        assert_eq!(mount.state, "active");

        let ocf = services.iter().find(|s| s.name.contains("ocf.rs")).unwrap();
        assert!(ocf.active);

        let mysql = services.iter().find(|s| s.name == "mysql.service").unwrap();
        assert!(mysql.active);
        assert_eq!(mysql.state, "active");

        let inactive = services
            .iter()
            .find(|s| s.name == "inactive.service")
            .unwrap();
        assert!(!inactive.active);
        assert_eq!(inactive.state, "inactive");

        let failed = services
            .iter()
            .find(|s| s.name == "failed.service")
            .unwrap();
        assert!(!failed.active);
        assert_eq!(failed.state, "failed");
    }
}

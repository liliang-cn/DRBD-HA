use crate::models::{ReactorProfileStatus, ReactorServiceDetail, ReactorServiceStatus};
use serde_json::Value;
use std::path::Path;

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

pub fn parse_reactor_status(output: &str, profile_name: Option<&str>) -> Vec<ReactorProfileStatus> {
    parse_reactor_status_json(output, profile_name).unwrap_or_default()
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
    parse_reactor_services_json(output).unwrap_or_default()
}

fn parse_reactor_status_json(
    output: &str,
    profile_name: Option<&str>,
) -> Option<Vec<ReactorProfileStatus>> {
    let root: Value = serde_json::from_str(output).ok()?;
    let obj = root.as_object()?;
    let local_hostname = gethostname::gethostname().to_string_lossy().to_string();
    let mut statuses = Vec::new();

    if let Some(promoters) = obj.get("promoter").and_then(Value::as_array) {
        for promoter in promoters {
            if !is_enabled(promoter) {
                continue;
            }
            let Some(path) = promoter.get("path").and_then(Value::as_str) else {
                continue;
            };
            let Some(name) = profile_name_from_path(path) else {
                continue;
            };

            if let Some(filter) = profile_name {
                if name != filter {
                    continue;
                }
            }

            let active_node = promoter
                .get("primary_on")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|node| !node.is_empty())
                .map(ToOwned::to_owned);

            statuses.push(ReactorProfileStatus {
                name,
                active_node: active_node.clone(),
                is_active: active_node.is_some(),
            });
        }
    }

    for (plugin_type, entries) in obj {
        if plugin_type == "promoter" {
            continue;
        }

        let Some(items) = entries.as_array() else {
            continue;
        };

        for item in items {
            if !is_enabled(item) {
                continue;
            }
            let Some(path) = item.get("path").and_then(Value::as_str) else {
                continue;
            };
            let Some(name) = profile_name_from_path(path) else {
                continue;
            };

            if let Some(filter) = profile_name {
                if name != filter {
                    continue;
                }
            }

            let is_active = item
                .get("status")
                .and_then(Value::as_str)
                .map(|status| status.eq_ignore_ascii_case("active"))
                .unwrap_or(false);

            statuses.push(ReactorProfileStatus {
                name,
                active_node: is_active.then(|| local_hostname.clone()),
                is_active,
            });
        }
    }

    Some(statuses)
}

fn parse_reactor_services_json(output: &str) -> Option<Vec<ReactorServiceStatus>> {
    let root: Value = serde_json::from_str(output).ok()?;
    let promoters = root.get("promoter")?.as_array()?;
    let mut services = Vec::new();

    for promoter in promoters {
        if !is_enabled(promoter) {
            continue;
        }
        let Some(dependencies) = promoter.get("dependencies").and_then(Value::as_array) else {
            continue;
        };

        for dependency in dependencies {
            push_json_service(dependency, &mut services);
        }
    }

    Some(services)
}

fn push_json_service(entry: &Value, services: &mut Vec<ReactorServiceStatus>) {
    let Some(name) = entry.get("name").and_then(Value::as_str).map(str::trim) else {
        return;
    };

    if name.is_empty() || name.starts_with("drbd-services@") || name.starts_with("drbd-promote@") {
        return;
    }

    let status = entry
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|status| !status.is_empty())
        .unwrap_or("inactive");

    let active = status.eq_ignore_ascii_case("active");
    let state = if status.eq_ignore_ascii_case("failed") {
        "failed".to_string()
    } else if active {
        "active".to_string()
    } else {
        status.to_string()
    };

    services.push(ReactorServiceStatus {
        name: name.to_string(),
        active,
        state,
    });
}

/// Whether a drbd-reactorctl status entry represents an enabled plugin.
///
/// From the next drbd-reactorctl release, `status --json` outputs both enabled
/// (`foo.toml`) and disabled (`foo.toml.disabled`) plugins, distinguished by an
/// `enabled` boolean. Older releases omit the field and only ever list enabled
/// plugins, so a missing field is treated as `enabled: true`.
fn is_enabled(entry: &Value) -> bool {
    entry
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

fn profile_name_from_path(path: &str) -> Option<String> {
    let file_name = Path::new(path).file_name()?.to_str()?;
    let profile_name = file_name
        .strip_suffix(".toml.disabled")
        .or_else(|| file_name.strip_suffix(".toml"))?;

    Some(profile_name.to_string())
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
    fn test_parse_reactor_status_invalid_json() {
        let output = "not-json";
        let statuses = parse_reactor_status(output, None);
        assert!(statuses.is_empty());
    }

    #[test]
    fn test_parse_reactor_status_skips_disabled() {
        // Next release of drbd-reactorctl emits both enabled and disabled
        // plugins, distinguished by an `enabled` field. A missing field means
        // enabled: true (current releases). Only enabled ones must be shown.
        let output = r#"{
  "promoter": [
    {
      "path": "/etc/drbd-reactor.d/enabled-explicit.toml",
      "primary_on": "node-a",
      "status": "active",
      "enabled": true
    },
    {
      "path": "/etc/drbd-reactor.d/disabled.toml.disabled",
      "status": "inactive",
      "enabled": false
    },
    {
      "path": "/etc/drbd-reactor.d/enabled-implicit.toml",
      "primary_on": "node-c",
      "status": "active"
    }
  ],
  "prometheus": [
    {
      "path": "/etc/drbd-reactor.d/prometheus.toml",
      "status": "active",
      "enabled": false
    }
  ]
}"#;

        let statuses = parse_reactor_status(output, None);
        let names: Vec<&str> = statuses.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"enabled-explicit"));
        assert!(names.contains(&"enabled-implicit"));
        assert!(
            !names.contains(&"disabled"),
            "disabled promoter must be filtered out"
        );
        assert!(
            !names.contains(&"prometheus"),
            "disabled plugin must be filtered out"
        );
        assert_eq!(statuses.len(), 2);
    }

    #[test]
    fn test_parse_reactor_services_skips_disabled_promoter() {
        let output = r#"{
  "promoter": [
    {
      "path": "/etc/drbd-reactor.d/disabled.toml.disabled",
      "enabled": false,
      "dependencies": [
        { "name": "service_ip_x.service", "status": "active" }
      ]
    }
  ]
}"#;
        let services = parse_reactor_services(output);
        assert!(
            services.is_empty(),
            "disabled promoter must contribute no services"
        );
    }

    #[test]
    fn test_parse_reactor_status_filtered() {
        let output = r#"{
  "promoter": [
    {
      "drbd_resource": "r1",
      "path": "/etc/drbd-reactor.d/profile-one.toml",
      "primary_on": "node-a",
      "target": {
        "name": "drbd-services@r1.target",
        "status": "active",
        "freezer": "running"
      },
      "dependencies": [],
      "status": "active"
    },
    {
      "drbd_resource": "r2",
      "path": "/etc/drbd-reactor.d/profile-two.toml",
      "primary_on": "node-b",
      "target": {
        "name": "drbd-services@r2.target",
        "status": "active",
        "freezer": "running"
      },
      "dependencies": [],
      "status": "active"
    }
  ]
}"#;
        let statuses = parse_reactor_status(output, Some("profile-one"));
        assert_eq!(statuses.len(), 1);
        let status = &statuses[0];
        assert_eq!(status.name, "profile-one");
        assert!(status.is_active);
        assert_eq!(status.active_node, Some("node-a".to_string()));
    }

    #[test]
    fn test_parse_service_details() {
        let output = r#"{
  "promoter": [
    {
      "drbd_resource": "r1",
      "path": "/etc/drbd-reactor.d/profile-one.toml",
      "primary_on": "node-a",
      "target": {
        "name": "drbd-services@r1.target",
        "status": "active",
        "freezer": "running"
      },
      "dependencies": [],
      "status": "active"
    },
    {
      "drbd_resource": "r2",
      "path": "/etc/drbd-reactor.d/profile-two.toml",
      "target": {
        "name": "drbd-services@r2.target",
        "status": "inactive",
        "freezer": "running"
      },
      "dependencies": [],
      "status": "inactive"
    }
  ]
}"#;
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
    fn test_parse_reactor_services_invalid_json() {
        let output = "not-json";
        let services = parse_reactor_services(output);
        assert!(services.is_empty());
    }

    #[test]
    fn test_parse_reactor_status_json() {
        let output = r#"{
  "promoter": [
    {
      "drbd_resource": "ha_mysql",
      "path": "/etc/drbd-reactor.d/mysql_config.toml",
      "primary_on": "gui01",
      "target": {
        "name": "drbd-services@ha_mysql.target",
        "status": "active",
        "freezer": "running"
      },
      "dependencies": [
        {
          "name": "drbd-promote@ha_mysql.service",
          "status": "active",
          "freezer": "running"
        },
        {
          "name": "ocf.rs@Dummy_new_ha_mysql.service",
          "status": "active",
          "freezer": "running"
        }
      ],
      "status": "active"
    },
    {
      "drbd_resource": "linstor_db",
      "path": "/etc/drbd-reactor.d/linstor_controller.toml",
      "primary_on": "gui03",
      "target": {
        "name": "drbd-services@linstor_db.target",
        "status": "inactive",
        "freezer": "running"
      },
      "dependencies": [],
      "status": "inactive"
    }
  ],
  "prometheus": [
    {
      "path": "/etc/drbd-reactor.d/prometheus.toml",
      "address": "0.0.0.0:9942",
      "status": "active"
    }
  ]
}"#;

        let statuses = parse_reactor_status(output, None);
        assert_eq!(statuses.len(), 3);

        let mysql = statuses.iter().find(|s| s.name == "mysql_config").unwrap();
        assert!(mysql.is_active);
        assert_eq!(mysql.active_node, Some("gui01".to_string()));

        let linstor = statuses
            .iter()
            .find(|s| s.name == "linstor_controller")
            .unwrap();
        assert!(linstor.is_active);
        assert_eq!(linstor.active_node, Some("gui03".to_string()));

        let prometheus = statuses.iter().find(|s| s.name == "prometheus").unwrap();
        assert!(prometheus.is_active);
        assert_eq!(
            prometheus.active_node,
            Some(gethostname().to_string_lossy().to_string())
        );
    }

    #[test]
    fn test_parse_reactor_services_json() {
        let output = r#"{
  "promoter": [
    {
      "drbd_resource": "nfs3",
      "path": "/etc/drbd-reactor.d/linstor-gateway-nfs-nfs3.toml",
      "primary_on": "gui02",
      "target": {
        "name": "drbd-services@nfs3.target",
        "status": "inactive",
        "freezer": "running"
      },
      "dependencies": [
        {
          "name": "drbd-promote@nfs3.service",
          "status": "inactive",
          "freezer": "running"
        },
        {
          "name": "ocf.rs@nfsserver_nfs3.service",
          "status": "failed",
          "freezer": "running"
        },
        {
          "name": "service_ip_nfs3.service",
          "status": "active",
          "freezer": "running"
        },
        {
          "name": "fs_1_nfs3.service",
          "status": "inactive",
          "freezer": "running"
        }
      ],
      "status": "inactive"
    }
  ]
}"#;

        let services = parse_reactor_services(output);
        assert_eq!(services.len(), 3);

        let active = services
            .iter()
            .find(|s| s.name == "service_ip_nfs3.service")
            .unwrap();
        assert!(active.active);
        assert_eq!(active.state, "active");

        let failed = services
            .iter()
            .find(|s| s.name == "ocf.rs@nfsserver_nfs3.service")
            .unwrap();
        assert!(!failed.active);
        assert_eq!(failed.state, "failed");

        let inactive = services
            .iter()
            .find(|s| s.name == "fs_1_nfs3.service")
            .unwrap();
        assert!(!inactive.active);
        assert_eq!(inactive.state, "inactive");
    }
}

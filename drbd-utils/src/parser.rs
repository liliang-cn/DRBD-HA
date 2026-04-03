use crate::models::{DrbdPeerStatus, DrbdResourceStatus, ResourceStatus};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::Command;

fn controller_proxy_from_env() -> Option<(String, u16, String)> {
    let host = std::env::var("DRBD_HA_REMOTE_EXEC_HOST").ok()?;
    let port = std::env::var("DRBD_HA_REMOTE_EXEC_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(22);
    let user = std::env::var("DRBD_HA_REMOTE_EXEC_USER").unwrap_or_else(|_| "root".to_string());
    Some((host, port, user))
}

fn shell_escape_single_quotes(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

/// Parse DRBD res file to extract node information (hostname and IP)
/// Returns a HashMap mapping hostname to IP address
/// Supports both standard DRBD format and LINSTOR-generated format
///
/// Standard format:
///   on "hostname" {
///     device minor 0;
///     address 10.43.7.11:7789;
///   }
///
/// LINSTOR format:
///   connection {
///     host "gui01" address ipv4 192.168.123.117:7006;
///     host "gui02" address ipv4 192.168.123.118:7006;
///   }
pub fn parse_res_file_for_nodes(content: &str) -> HashMap<String, String> {
    let mut nodes = HashMap::new();

    // First pass: try standard DRBD format
    // Parse lines like "on hostname {" and "address ip:port;"
    let mut current_hostname: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        // Match "on hostname {" (quoted or unquoted)
        if trimmed.starts_with("on ") && trimmed.ends_with('{') {
            let hostname_part = trimmed
                .strip_prefix("on ")
                .unwrap_or("")
                .trim_end_matches('{')
                .trim()
                .trim_start_matches('"')
                .trim_end_matches('"');
            if !hostname_part.is_empty() {
                current_hostname = Some(hostname_part.to_string());
            }
        }
        // Match "address ip:port;"
        else if trimmed.starts_with("address ") && trimmed.ends_with(';') {
            let address_part = trimmed
                .strip_prefix("address ")
                .unwrap_or("")
                .trim_end_matches(';')
                .trim();

            // Extract IP from "ip:port" format
            if let Some(ip_with_port) = address_part.split(':').next() {
                if let Some(hostname) = current_hostname.clone() {
                    nodes.insert(hostname, ip_with_port.to_string());
                }
            }
        }
        // Reset on closing brace
        else if trimmed == "}" {
            current_hostname = None;
        }
    }

    // If no nodes found in standard format, try LINSTOR format
    if nodes.is_empty() {
        let mut in_connection = false;
        let mut current_hostname: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();

            // Start of connection block
            if trimmed == "connection" || trimmed == "connection{" {
                in_connection = true;
                continue;
            }

            // End of connection block
            if trimmed == "}" || (trimmed.starts_with('}') && in_connection) {
                in_connection = false;
                current_hostname = None;
                continue;
            }

            // Only parse within connection blocks
            if !in_connection {
                continue;
            }

            // Match: host "hostname" address ipv4 ip:port;
            if trimmed.contains("host ") && trimmed.contains("address") {
                // Extract hostname
                if let Some(start) = trimmed.find("host ") {
                    let after_host = &trimmed[start + 5..];
                    // Find quoted hostname - find first quote, then second quote
                    if let Some(first_quote) = after_host.find('"') {
                        let after_first_quote = &after_host[first_quote + 1..];
                        if let Some(second_quote) = after_first_quote.find('"') {
                            let hostname = &after_first_quote[..second_quote];
                            current_hostname = Some(hostname.to_string());
                        }
                    }
                }

                // Extract IP from "address ipv4 ip:port;" or "address ipv6 ip:port;"
                if let Some(addr_start) = trimmed.find("address") {
                    let after_addr = &trimmed[addr_start..];
                    // Skip "address" and find "ipv4" or "ipv6"
                    if let Some(ipv_start) = after_addr.find("ipv4").or(after_addr.find("ipv6")) {
                        let after_ipv = &after_addr[ipv_start + 4..];
                        let ip_port = after_ipv.trim();
                        // Extract IP from "ip:port;" format
                        if let Some(semicolon_pos) = ip_port.find(';') {
                            let ip_with_port = &ip_port[..semicolon_pos];
                            if let Some(ip) = ip_with_port.split(':').next() {
                                if let Some(hostname) = current_hostname.clone() {
                                    nodes.insert(hostname, ip.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    nodes
}

/// Parse DRBD res file to extract minor numbers from device lines
/// Returns a HashSet of used minor numbers
///
/// Supports formats:
///   device /dev/drbd0 minor 0;
///   device /dev/drbd1;
pub fn parse_res_file_for_minors(content: &str) -> HashSet<u32> {
    let mut minors = HashSet::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Match "device /dev/drbdX" where X is the minor number
        if trimmed.starts_with("device ") && trimmed.ends_with(';') {
            let device_part = trimmed
                .strip_prefix("device ")
                .unwrap_or("")
                .trim_end_matches(';')
                .trim();

            // Extract minor number from /dev/drbdX
            if device_part.starts_with("/dev/drbd") {
                let after_drbd = device_part.strip_prefix("/dev/drbd").unwrap_or("");
                // Parse the number after "drbd"
                if let Ok(minor) = after_drbd.parse::<u32>() {
                    minors.insert(minor);
                }
            }
        }
    }

    minors
}

/// Get all used minor numbers from existing .res files in /etc/drbd.d/
/// Returns a HashSet of used minor numbers
pub fn get_used_minors_from_config() -> HashSet<u32> {
    let mut used_minors = HashSet::new();
    let config_dir = Path::new("/etc/drbd.d");

    if let Some((host, port, user)) = controller_proxy_from_env() {
        let target = format!("{}@{}", user, host);
        let escaped_dir = shell_escape_single_quotes(&config_dir.to_string_lossy());
        let list_cmd = if user == "root" {
            format!("find '{}' -maxdepth 1 -type f -name '*.res' -print", escaped_dir)
        } else {
            format!(
                "sudo -n find '{}' -maxdepth 1 -type f -name '*.res' -print",
                escaped_dir
            )
        };

        let list_output = Command::new("ssh")
            .arg("-p")
            .arg(port.to_string())
            .arg("-o")
            .arg("StrictHostKeyChecking=no")
            .arg("-o")
            .arg("UserKnownHostsFile=/dev/null")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ConnectTimeout=5")
            .arg(&target)
            .arg(&list_cmd)
            .output();

        if let Ok(output) = list_output {
            if output.status.success() {
                for file_path in String::from_utf8_lossy(&output.stdout).lines() {
                    let file_path = file_path.trim();
                    if file_path.is_empty() {
                        continue;
                    }

                    let escaped_path = shell_escape_single_quotes(file_path);
                    let cat_cmd = if user == "root" {
                        format!("cat '{}'", escaped_path)
                    } else {
                        format!("sudo -n cat '{}'", escaped_path)
                    };

                    if let Ok(content_output) = Command::new("ssh")
                        .arg("-p")
                        .arg(port.to_string())
                        .arg("-o")
                        .arg("StrictHostKeyChecking=no")
                        .arg("-o")
                        .arg("UserKnownHostsFile=/dev/null")
                        .arg("-o")
                        .arg("BatchMode=yes")
                        .arg("-o")
                        .arg("ConnectTimeout=5")
                        .arg(&target)
                        .arg(&cat_cmd)
                        .output()
                    {
                        if content_output.status.success() {
                            let content = String::from_utf8_lossy(&content_output.stdout);
                            let minors = parse_res_file_for_minors(&content);
                            used_minors.extend(minors);
                        }
                    }
                }
            }
        }

        return used_minors;
    }

    if !config_dir.exists() {
        return used_minors;
    }

    // Read all .res files in the directory
    if let Ok(entries) = fs::read_dir(config_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("res") {
                if let Ok(content) = fs::read_to_string(&path) {
                    let minors = parse_res_file_for_minors(&content);
                    used_minors.extend(minors);
                }
            }
        }
    }

    used_minors
}

/// Allocate a new minor number for a DRBD resource
/// System-created resources start from 2000 and increment
/// Returns the next available minor number
pub fn allocate_minor() -> u32 {
    const SYSTEM_MINOR_START: u32 = 2000;
    let used_minors = get_used_minors_from_config();

    // Start from 2000 and find the first available minor
    let mut minor = SYSTEM_MINOR_START;
    while used_minors.contains(&minor) {
        minor += 1;
    }

    minor
}

/// Parse drbdadm status output into structured format
pub fn parse_drbdadm_status(output: &str, resource_name: &str) -> Option<DrbdResourceStatus> {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return None;
    }

    // First line: "mongodb-data role:Primary"
    let first_line = lines.first()?;
    if !first_line.starts_with(resource_name) {
        return None;
    }

    // Parse role from first line
    let role = first_line
        .split_whitespace()
        .find(|s| s.starts_with("role:"))
        .and_then(|s| s.strip_prefix("role:"))
        .unwrap_or("Unknown")
        .to_string();

    // Parse local disk status and open state from second line
    // "  disk:UpToDate open:yes"
    let (disk, open) = if lines.len() > 1 {
        let disk_line = lines[1].trim();
        let disk = disk_line
            .split_whitespace()
            .find(|s| s.starts_with("disk:"))
            .and_then(|s| s.strip_prefix("disk:"))
            .unwrap_or("Unknown")
            .to_string();
        let open = disk_line
            .split_whitespace()
            .find(|s| s.starts_with("open:"))
            .and_then(|s| s.strip_prefix("open:"))
            .map(|s| s == "yes")
            .unwrap_or(false);
        (disk, open)
    } else {
        ("Unknown".to_string(), false)
    };

    // Parse peer nodes
    let mut peers = Vec::new();
    let mut i = 2;
    while i < lines.len() {
        let line = lines[i].trim();

        // Peer line format: "gui02 role:Secondary" or "gui02 role:Secondary connection:Connected"
        if !line.starts_with("peer-disk:") && !line.is_empty() && !line.starts_with("disk:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if !parts.is_empty() {
                let peer_name = parts[0].to_string();

                // Skip if it looks like a status field rather than a hostname
                if peer_name.contains(':') {
                    i += 1;
                    continue;
                }

                let peer_role = parts
                    .iter()
                    .find(|s| s.starts_with("role:"))
                    .and_then(|s| s.strip_prefix("role:"))
                    .unwrap_or("Unknown")
                    .to_string();

                let connection = parts
                    .iter()
                    .find(|s| s.starts_with("connection:"))
                    .and_then(|s| s.strip_prefix("connection:"))
                    .map(|s| s.to_string());

                let mut replication = parts
                    .iter()
                    .find(|s| s.starts_with("replication:"))
                    .and_then(|s| s.strip_prefix("replication:"))
                    .map(|s| s.to_string());

                let mut sync_percent = parts
                    .iter()
                    .find(|s| s.starts_with("done:"))
                    .and_then(|s| s.strip_prefix("done:"))
                    .and_then(|s| s.parse::<f64>().ok());

                // Next line should have peer-disk or other details
                let mut peer_disk = "Unknown".to_string();
                if i + 1 < lines.len() {
                    let next_line = lines[i + 1].trim();
                    // We check next line if it seems to contain peer info (peer-disk or replication)
                    if next_line.starts_with("peer-disk:")
                        || next_line.contains("replication:")
                        || next_line.contains("done:")
                    {
                        let next_parts: Vec<&str> = next_line.split_whitespace().collect();

                        if let Some(pd) = next_parts.iter().find(|s| s.starts_with("peer-disk:")) {
                            peer_disk = pd
                                .strip_prefix("peer-disk:")
                                .unwrap_or("Unknown")
                                .to_string();
                        }

                        if replication.is_none() {
                            replication = next_parts
                                .iter()
                                .find(|s| s.starts_with("replication:"))
                                .and_then(|s| s.strip_prefix("replication:"))
                                .map(|s| s.to_string());
                        }

                        if sync_percent.is_none() {
                            sync_percent = next_parts
                                .iter()
                                .find(|s| s.starts_with("done:"))
                                .and_then(|s| s.strip_prefix("done:"))
                                .and_then(|s| s.parse::<f64>().ok());
                        }
                    }
                }

                // If connection state is not explicitly shown, but peer is listed,
                // assume Connected (peer wouldn't be listed if not connected)
                let connection_state = connection.or_else(|| {
                    if !peer_name.is_empty() {
                        Some("Connected".to_string())
                    } else {
                        None
                    }
                });

                peers.push(DrbdPeerStatus {
                    name: peer_name,
                    role: peer_role,
                    peer_disk,
                    connection: connection_state,
                    replication,
                    sync_percent,
                });
            }
        }
        i += 1;
    }

    Some(DrbdResourceStatus {
        resource: resource_name.to_string(),
        role,
        disk,
        open,
        minor: None, // Not available in text format
        peers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_drbdadm_status_primary_up_to_date() {
        let output = r#"mysql_data role:Primary
  disk:UpToDate open:yes
  node2 role:Secondary
    peer-disk:UpToDate"#;
        let status = parse_drbdadm_status(output, "mysql_data").unwrap();
        assert_eq!(status.resource, "mysql_data");
        assert_eq!(status.role, "Primary");
        assert_eq!(status.disk, "UpToDate");
        assert!(status.open);
        assert_eq!(status.peers.len(), 1);
        assert_eq!(status.peers[0].name, "node2");
        assert_eq!(status.peers[0].role, "Secondary");
        assert_eq!(status.peers[0].peer_disk, "UpToDate");
    }

    #[test]
    fn test_parse_drbdadm_status_secondary_inconsistent() {
        let output = r#"r0 role:Secondary
  disk:Inconsistent
  node1 role:Primary
    peer-disk:UpToDate"#;
        let status = parse_drbdadm_status(output, "r0").unwrap();
        assert_eq!(status.resource, "r0");
        assert_eq!(status.role, "Secondary");
        assert_eq!(status.disk, "Inconsistent");
        assert!(!status.open);
        assert_eq!(status.peers.len(), 1);
        assert_eq!(status.peers[0].name, "node1");
        assert_eq!(status.peers[0].role, "Primary");
        assert_eq!(status.peers[0].peer_disk, "UpToDate");
    }

    #[test]
    fn test_parse_drbdadm_status_syncing() {
        let output = r#"postgres_data role:Primary
  disk:UpToDate open:no
  orange2 role:Secondary
    replication:SyncSource peer-disk:Inconsistent done:19.30
  orange3 role:Secondary
    replication:SyncSource peer-disk:Inconsistent done:19.20"#;
        let status = parse_drbdadm_status(output, "postgres_data").unwrap();
        assert_eq!(status.resource, "postgres_data");
        assert_eq!(status.role, "Primary");
        assert_eq!(status.disk, "UpToDate");
        assert!(!status.open);
        assert_eq!(status.peers.len(), 2);

        let p1 = &status.peers[0];
        assert_eq!(p1.name, "orange2");
        assert_eq!(p1.role, "Secondary");
        assert_eq!(p1.peer_disk, "Inconsistent");
        assert_eq!(p1.replication.as_deref(), Some("SyncSource"));
        assert_eq!(p1.sync_percent, Some(19.30));

        let p2 = &status.peers[1];
        assert_eq!(p2.name, "orange3");
        assert_eq!(p2.role, "Secondary");
        assert_eq!(p2.peer_disk, "Inconsistent");
        assert_eq!(p2.replication.as_deref(), Some("SyncSource"));
        assert_eq!(p2.sync_percent, Some(19.20));
    }

    #[test]
    fn test_convert_resource_status_to_drbd_status() {
        use crate::models::{ConnectionStatus, DeviceStatus, PeerDeviceStatus};

        let resource = ResourceStatus {
            name: "mysql_data".to_string(),
            role: "Primary".to_string(),
            devices: vec![DeviceStatus {
                volume: 0,
                disk_state: "UpToDate".to_string(),
                minor: 0,
                size: Some(20970204),
            }],
            connections: vec![ConnectionStatus {
                peer_node_id: 2,
                name: "orange2".to_string(),
                connection_state: "Connected".to_string(),
                peer_role: Some("Secondary".to_string()),
                peer_devices: vec![PeerDeviceStatus {
                    volume: 0,
                    replication_state: "Established".to_string(),
                    peer_disk_state: "UpToDate".to_string(),
                    percent_in_sync: Some(100.0),
                }],
            }],
        };

        let drbd_status = convert_resource_status(&resource);
        assert_eq!(drbd_status.resource, "mysql_data");
        assert_eq!(drbd_status.role, "Primary");
        assert_eq!(drbd_status.disk, "UpToDate");
        assert_eq!(drbd_status.peers.len(), 1);
        assert_eq!(drbd_status.peers[0].name, "orange2");
        assert_eq!(drbd_status.peers[0].role, "Secondary");
        assert_eq!(
            drbd_status.peers[0].connection,
            Some("Connected".to_string())
        );
        assert_eq!(
            drbd_status.peers[0].replication,
            Some("Established".to_string())
        );
    }
}

/// Convert ResourceStatus (from drbdadm status --json) to DrbdResourceStatus
///
/// This function converts the JSON-parsed ResourceStatus into the simpler
/// DrbdResourceStatus format used by the HA profile API.
pub fn convert_resource_status(resource: &ResourceStatus) -> DrbdResourceStatus {
    let peers = resource
        .connections
        .iter()
        .map(|conn| {
            let peer_device = conn.peer_devices.first();
            DrbdPeerStatus {
                name: conn.name.clone(),
                role: conn.peer_role.clone().unwrap_or_else(|| {
                    // Try to infer from replication state
                    if peer_device
                        .map(|p| p.replication_state.contains("Target"))
                        .unwrap_or(false)
                    {
                        "Secondary".to_string()
                    } else {
                        "Unknown".to_string()
                    }
                }),
                peer_disk: peer_device
                    .map(|p| p.peer_disk_state.clone())
                    .unwrap_or_else(|| "Unknown".to_string()),
                connection: Some(conn.connection_state.clone()),
                replication: peer_device.map(|p| p.replication_state.clone()),
                sync_percent: peer_device.and_then(|p| p.percent_in_sync),
            }
        })
        .collect();

    DrbdResourceStatus {
        resource: resource.name.clone(),
        role: resource.role.clone(),
        disk: resource
            .devices
            .first()
            .map(|d| d.disk_state.clone())
            .unwrap_or_else(|| "Unknown".to_string()),
        open: resource
            .devices
            .first()
            .map(|d| d.size.is_some())
            .unwrap_or(false),
        minor: resource.devices.first().map(|d| d.minor),
        peers,
    }
}

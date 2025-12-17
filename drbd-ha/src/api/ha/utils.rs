use crate::error::AppResult;
use crate::models::{Node, VipConfig};
use crate::state::AppState;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::fs;

/// Get SSH credential for a node (Dummy) - copied from cluster.rs
pub(crate) async fn get_node_credential(
    _state: &Arc<AppState>,
    _node: &Node,
) -> AppResult<Option<crate::core::SshCredential>> {
    // We don't use credentials anymore, just return a dummy one
    Ok(Some(crate::core::SshCredential::Password(
        "ignored".to_string(),
    )))
}

/// Parse VIP configuration from drbd-reactor config file content
pub(crate) fn parse_vip_from_config(content: &str) -> Option<VipConfig> {
    // Find lines containing IPaddr2
    for line in content.lines() {
        if line.contains("ocf:heartbeat:IPaddr2") {
            // Extract the service definition from TOML array (between quotes)
            let service_def = if let Some(start) = line.find("ocf:heartbeat:IPaddr2") {
                // Find the end quote after IPaddr2
                let after_ipaddr2 = &line[start..];
                if let Some(end_quote) = after_ipaddr2.find('"') {
                    &after_ipaddr2[..end_quote]
                } else {
                    after_ipaddr2
                }
            } else {
                continue;
            };

            // Extract ip= and cidr_netmask= parameters
            let mut ip_addr = None;
            let mut netmask = None;

            for part in service_def.split_whitespace() {
                if let Some(addr) = part.strip_prefix("ip=") {
                    ip_addr = Some(addr.to_string());
                } else if let Some(mask) = part.strip_prefix("cidr_netmask=") {
                    netmask = mask.parse::<u8>().ok();
                }
            }

            if let (Some(address), Some(netmask)) = (ip_addr, netmask) {
                let interface = "eth0".to_string();

                return Some(VipConfig {
                    address,
                    netmask,
                    interface,
                });
            }
        }
    }

    None
}

/// Parse mount point from drbd-reactor config file content
pub(crate) fn parse_mount_point_from_config(content: &str) -> Option<String> {
    // Find lines with .mount units
    for line in content.lines() {
        if line.contains(".mount") {
            // Extract mount unit name from TOML array
            for part in line.split('"') {
                if part.ends_with(".mount") {
                    // Convert systemd mount unit name to path
                    // Example: var-lib-mongodb.mount -> /var/lib/mongodb
                    let name = part.strip_suffix(".mount")?;
                    return Some(format!("/{}", name.replace('-', "/")));
                }
            }
        }
    }
    None
}

/// Find the next available DRBD minor number by scanning /etc/drbd.d/*.res
pub async fn find_next_free_drbd_minor() -> AppResult<u32> {
    let mut used_minors = HashSet::new();
    let config_dir = "/etc/drbd.d";

    if let Ok(mut entries) = fs::read_dir(config_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "res") {
                if let Ok(content) = fs::read_to_string(&path).await {
                    for line in content.lines() {
                        let line = line.trim();
                        if line.starts_with("minor") {
                            if let Some(val) = line.split_whitespace().nth(1) {
                                let val = val.trim_matches(';');
                                if let Ok(n) = val.parse::<u32>() {
                                    used_minors.insert(n);
                                }
                            }
                        } else if line.starts_with("device") {
                            // device /dev/drbd0;
                            if let Some(dev) = line.split_whitespace().nth(1) {
                                let dev = dev.trim_matches(';');
                                if let Some(num_str) = dev.strip_prefix("/dev/drbd") {
                                    if let Ok(n) = num_str.parse::<u32>() {
                                        used_minors.insert(n);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut minor = 0;
    while used_minors.contains(&minor) {
        minor += 1;
    }
    Ok(minor)
}

/// Find the next available DRBD port by scanning /etc/drbd.d/*.res
pub async fn find_next_free_drbd_port() -> AppResult<u16> {
    let mut used_ports = HashSet::new();
    let config_dir = "/etc/drbd.d";

    if let Ok(mut entries) = fs::read_dir(config_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "res") {
                if let Ok(content) = fs::read_to_string(&path).await {
                    for line in content.lines() {
                        let line = line.trim();
                        // address 192.168.1.1:7789;
                        if line.starts_with("address") {
                            if let Some(addr_part) = line.split_whitespace().nth(1) {
                                let addr_part = addr_part.trim_matches(';');
                                if let Some(idx) = addr_part.rfind(':') {
                                    if let Ok(port) = addr_part[idx + 1..].parse::<u16>() {
                                        used_ports.insert(port);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut port = 7789;
    while used_ports.contains(&port) {
        port += 1;
    }
    Ok(port)
}

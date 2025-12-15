use crate::error::AppResult;
use crate::models::{Node, VipConfig};
use crate::state::AppState;
use std::sync::Arc;

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
/// Example: start = ["...", "ocf:heartbeat:IPaddr2 vip cidr_netmask=24 ip=192.168.123.198", "..."]
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
/// Example: start = ["var-lib-mongodb.mount", ...] -> "/var/lib/mongodb"
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

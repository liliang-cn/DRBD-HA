use crate::models::{
    GeneratedUnits, HaProfile, HaProfileStatus, HaType, MountStrategy, Node, NodeStatus,
    OcfAgentConfig, PromoterSettings, VipConfig,
};
use anyhow::Result;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::warn;

#[derive(Deserialize, Debug)]
struct ReactorToml {
    promoter: Option<Vec<PromoterSection>>,
}

#[derive(Deserialize, Debug)]
struct PromoterSection {
    id: Option<String>,
    resources: HashMap<String, ResourceSection>,
    #[allow(dead_code)]
    runner: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ResourceSection {
    start: Option<Vec<String>>,
    #[serde(rename = "stop-services-on-exit")]
    stop_services_on_exit: Option<bool>,
    #[serde(rename = "on-drbd-demote-failure")]
    on_demote_failure: Option<String>,
    #[serde(rename = "preferred-nodes")]
    preferred_nodes: Option<Vec<String>>,
    #[serde(rename = "preferred-nodes-policy")]
    preferred_nodes_policy: Option<String>,
    #[serde(rename = "sleep-before-promote-factor")]
    sleep_before_promote_factor: Option<u32>,
    #[serde(rename = "dependencies-as")]
    dependencies_as: Option<String>,
    #[serde(rename = "target-as")]
    target_as: Option<String>,
    #[serde(rename = "on-quorum-loss")]
    on_quorum_loss: Option<String>,
}

pub struct ReactorDiscovery;

impl ReactorDiscovery {
    pub const REACTOR_CONFIG_DIR: &'static str = "/etc/drbd-reactor.d";
    pub const DRBD_CONFIG_DIR: &'static str = "/etc/drbd.d";

    /// Scan for profiles in the configuration directory
    pub fn scan_profiles() -> Result<Vec<HaProfile>> {
        let dir = Self::REACTOR_CONFIG_DIR;
        let path = Path::new(dir);
        let mut discovered = Vec::new();

        if !path.exists() {
            return Ok(discovered);
        }

        // Pre-compile regexes
        let vip_regex = Regex::new(
            r"ocf:heartbeat:IPaddr2\s+\S+\s+ip=([0-9\.]+)\s+cidr_netmask=(\d+)"
        )
        .unwrap();
        let fs_regex = Regex::new(
            r"ocf:heartbeat:Filesystem\s+\S+\s+device=(\S+)\s+directory=(\S+)\s+fstype=(\S+)"
        )
        .unwrap();
        let agent_regex = Regex::new(r"^(\S+)\s+(\S+)(?:\s+(.*))?$").unwrap();

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "toml") {
                let content = match fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("Failed to read {}: {}", path.display(), e);
                        continue;
                    }
                };

                // Attempt to parse
                let toml: ReactorToml = match toml::from_str(&content) {
                    Ok(t) => t,
                    Err(e) => {
                        warn!("Failed to parse TOML structure in {}: {}", path.display(), e);
                        continue;
                    }
                };

                if let Some(promoters) = toml.promoter {
                    for promoter in promoters {
                        // We assume one resource per promoter usually
                        if let Some((res_name, res_conf)) = promoter.resources.into_iter().next() {
                            let mut services: Vec<String> = Vec::new();
                            let mut vip: Option<VipConfig> = None;
                            let mut ocf_agents: Vec<OcfAgentConfig> = Vec::new();
                            let mut mount_strategy = MountStrategy::Systemd;
                            let mut mount_point = String::new();
                            let mut fs_type = "xfs".to_string();
                            let mut generated_mount_unit = None;

                            let start_list = res_conf.start.unwrap_or_default();

                            for item in &start_list {
                                // Check for VIP
                                if let Some(caps) = vip_regex.captures(item) {
                                    vip = Some(VipConfig {
                                        address: caps[1].to_string(),
                                        netmask: caps[2].parse().unwrap_or(24),
                                    });
                                    continue;
                                }

                                // Check for Filesystem (OCF Mount)
                                if let Some(caps) = fs_regex.captures(item) {
                                    mount_strategy = MountStrategy::Ocf;
                                    // device is caps[1]
                                    mount_point = caps[2].to_string();
                                    fs_type = caps[3].to_string();
                                    continue;
                                }

                                // Check for other OCF agents
                                if item.starts_with("ocf:") || item.starts_with("lsb:") || item.starts_with("service:") {
                                     if let Some(caps) = agent_regex.captures(item) {
                                         let name = caps[1].to_string();
                                         let instance_name = caps[2].to_string();
                                         let params_str = caps.get(3).map(|m| m.as_str()).unwrap_or("");
                                         
                                         let mut params = HashMap::new();
                                         for pair in params_str.split_whitespace() {
                                             if let Some((k, v)) = pair.split_once('=') {
                                                 params.insert(k.to_string(), v.to_string());
                                             }
                                         }
                                         
                                         ocf_agents.push(OcfAgentConfig {
                                             name,
                                             instance_name,
                                             params,
                                         });
                                     }
                                     continue;
                                }
                                
                                // Check for systemd mount unit
                                if item.ends_with(".mount") {
                                    generated_mount_unit = Some(item.clone());
                                    // Try to guess mount point from unit name (e.g. var-lib-mysql.mount -> /var/lib/mysql)
                                    // This is loose, but mostly visual.
                                    if mount_point.is_empty() {
                                        let path_str = item.trim_end_matches(".mount").replace('-', "/");
                                         mount_point = format!("/{}", path_str.trim_start_matches('/'));
                                    }
                                    continue; // Treated as part of the "start" sequence but handled specially
                                }

                                // Normal services
                                services.push(item.clone());
                            }

                            // Heuristics to detect HA Type
                            let ha_type = HaType::Generic;

                            // Use resource name as ID for stability
                            let id = res_name.clone();
                            let name = promoter.id.clone().unwrap_or_else(|| res_name.clone());

                            let profile = HaProfile {
                                id,
                                name,
                                ha_type,
                                resource_name: res_name,
                                mount_point,
                                fs_type,
                                mount_strategy,
                                vip,
                                ocf_agents,
                                promoter: PromoterSettings {
                                    services,
                                    stop_on_demote: res_conf.stop_services_on_exit.unwrap_or(true),
                                    on_demote_failure: res_conf.on_demote_failure.unwrap_or_else(|| "reboot".to_string()),
                                    preferred_nodes: res_conf.preferred_nodes,
                                    preferred_nodes_policy: res_conf.preferred_nodes_policy,
                                    sleep_before_promote_factor: res_conf.sleep_before_promote_factor,
                                    dependencies_as: res_conf.dependencies_as,
                                    target_as: res_conf.target_as,
                                    on_quorum_loss: res_conf.on_quorum_loss,
                                },
                                status: HaProfileStatus::Unknown, // Will be updated by runtime check
                                active_node: None,
                                generated_units: GeneratedUnits {
                                    mount_unit: generated_mount_unit,
                                    ..Default::default()
                                },
                            };
                            discovered.push(profile);
                        }
                    }
                }
            }
        }
        Ok(discovered)
    }

    /// Scan DRBD resource files to discover nodes
    pub fn scan_nodes() -> Result<Vec<Node>> {
        let dir = Self::DRBD_CONFIG_DIR;
        let path = Path::new(dir);
        let mut nodes = HashMap::new();

        if !path.exists() {
            return Ok(Vec::new());
        }
        
        // Regex to find "on <hostname> { ... address <ip>:<port>"
        // This is a multi-line match, so we need to be careful or read the whole file.
        // Simplified regex for line-by-line or block parsing.
        // We'll read the whole file content.
        let node_regex = Regex::new(r"on\s+([a-zA-Z0-9\-\.]+)\s+\{[^}]*address\s+([0-9\.]+):(\d+);" ).unwrap();

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "res") {
                let content = match fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("Failed to read {}: {}", path.display(), e);
                        continue;
                    }
                };
                
                // remove comments
                let cleaned_content = content.lines()
                    .filter(|l| !l.trim().starts_with('#'))
                    .collect::<Vec<_>>()
                    .join("\n");

                for caps in node_regex.captures_iter(&cleaned_content) {
                    let hostname = caps[1].to_string();
                    let ip = caps[2].to_string();
                    // let port = caps[3].to_string();

                    nodes.entry(hostname.clone()).or_insert_with(|| Node {
                        id: hostname.clone(),
                        hostname,
                        ip,
                        ssh_port: 22, // Default assumption
                        ssh_user: "root".to_string(), // Default assumption
                        is_local: false, // Will be checked later
                        status: NodeStatus::Unknown,
                        last_seen: None,
                    });
                }
            }
        }
        
        Ok(nodes.into_values().collect())
    }

    /// Get a specific profile by name
    pub fn get_profile(name: &str) -> Result<Option<HaProfile>> {
        let profiles = Self::scan_profiles()?;
        Ok(profiles.into_iter().find(|p| p.name == name))
    }

    /// Check if a profile exists
    pub fn profile_exists(name: &str) -> Result<bool> {
        let profiles = Self::scan_profiles()?;
        Ok(profiles.iter().any(|p| p.name == name))
    }
}
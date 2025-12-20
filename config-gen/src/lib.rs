//! Configuration file generator
//!
//! Generates DRBD resource files and drbd-reactor TOML configurations.

use serde::Serialize;
use std::collections::HashMap;
use tera::{Context, Tera};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to load template: {0}")]
    LoadTemplate(String),
    #[error("Failed to render template: {0}")]
    Render(String),
}

pub type Result<T> = std::result::Result<T, ConfigError>;

/// Configuration generator using Tera templates
pub struct ConfigGenerator {
    tera: Tera,
}

/// Node configuration for DRBD resource
#[derive(Debug, Clone, Serialize)]
pub struct NodeConfig {
    pub hostname: String,
    pub ip: String,
    pub disk: String,
    pub node_id: u32,
}

/// DRBD resource configuration
#[derive(Debug, Clone, Serialize)]
pub struct ResourceConfig {
    pub name: String,
    pub port: u16,
    pub minor: u32,
    pub device: String,
    pub nodes: Vec<NodeConfig>,
    pub disk_options: HashMap<String, String>,
    pub net_options: HashMap<String, String>,
    /// Whether to enable auto-promote (set to false for drbd-reactor managed resources)
    pub auto_promote: bool,
}

impl Default for ResourceConfig {
    fn default() -> Self {
        let mut disk_options = HashMap::new();
        disk_options.insert("on-io-error".to_string(), "detach".to_string());

        let mut net_options = HashMap::new();
        net_options.insert("protocol".to_string(), "C".to_string());
        net_options.insert("verify-alg".to_string(), "sha256".to_string());

        Self {
            name: String::new(),
            port: 7789,
            minor: 0,
            device: "/dev/drbd0".to_string(),
            nodes: Vec::new(),
            disk_options,
            net_options,
            auto_promote: true, // Default to true, set to false for drbd-reactor managed resources
        }
    }
}

/// Promoter configuration for drbd-reactor
#[derive(Debug, Clone, Serialize)]
pub struct PromoterPluginConfig {
    pub resource: String,
    /// Mount unit to start before services (e.g., "var-lib-mysql.mount")
    pub mount_unit: Option<String>,
    pub start: Vec<String>,
    pub stop_services_on_exit: bool,
    pub on_drbd_demote_failure: String,
    pub vip: Option<VipPluginConfig>,
    /// Generic OCF Agents to start
    #[serde(default)]
    pub ocf_agents: Vec<OcfAgentConfig>,
    /// Mount strategy: "systemd" (default) or "ocf"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_strategy: Option<String>,
    /// Mount point (required for OCF strategy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_point: Option<String>,
    /// Filesystem type (required for OCF strategy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fs_type: Option<String>,

    // --- Advanced Options ---
    pub dependencies_as: Option<String>,
    pub target_as: Option<String>,
    pub on_quorum_loss: Option<String>,
    pub preferred_nodes: Option<Vec<String>>,
    pub preferred_nodes_policy: Option<String>,
    pub sleep_before_promote_factor: Option<u32>,
}

/// OCF Agent configuration
#[derive(Debug, Clone, Serialize)]
pub struct OcfAgentConfig {
    pub name: String,          // e.g., "ocf:heartbeat:IPaddr2"
    pub instance_name: String, // e.g., "r0_vip"
    pub params: HashMap<String, String>,
}

/// VIP configuration for drbd-reactor promoter
#[derive(Debug, Clone, Serialize)]
pub struct VipPluginConfig {
    pub address: String,
    pub netmask: u8,
    pub interface: String,
}

impl ConfigGenerator {
    /// Create a new configuration generator
    pub fn new() -> Result<Self> {
        let mut tera = Tera::default();

        // Add DRBD resource template
        tera.add_raw_template("drbd_resource.res", DRBD_RESOURCE_TEMPLATE)
            .map_err(|e| ConfigError::LoadTemplate(e.to_string()))?;

        // Add drbd-reactor promoter template
        tera.add_raw_template("promoter.toml", PROMOTER_TEMPLATE)
            .map_err(|e| ConfigError::LoadTemplate(e.to_string()))?;

        Ok(Self { tera })
    }

    /// Generate DRBD resource configuration file content
    pub fn generate_drbd_resource(&self, config: &ResourceConfig) -> Result<String> {
        let mut context = Context::new();
        context.insert("resource", config);

        self.tera
            .render("drbd_resource.res", &context)
            .map_err(|e| ConfigError::Render(e.to_string()))
    }

    /// Generate drbd-reactor promoter configuration
    pub fn generate_promoter(&self, config: &PromoterPluginConfig) -> Result<String> {
        let mut context = Context::new();
        context.insert("promoter", config);

        self.tera
            .render("promoter.toml", &context)
            .map_err(|e| ConfigError::Render(e.to_string()))
    }
}

impl Default for ConfigGenerator {
    fn default() -> Self {
        Self::new().expect("Failed to create config generator")
    }
}

/// DRBD resource configuration template
const DRBD_RESOURCE_TEMPLATE: &str = r#"# DRBD resource configuration
# Generated by drbd-ha

resource {{ resource.name }} {
    options {
        auto-promote {% if resource.auto_promote %}yes{% else %}no{% endif %};
        quorum majority;
        on-no-quorum io-error;
{% if not resource.auto_promote %}
        on-suspended-primary-outdated force-secondary;
        on-no-data-accessible io-error;
{% endif %}
    }

    disk {
{% for key, value in resource.disk_options %}
        {{ key }} {{ value }};
{% endfor %}
    }

    net {
{% for key, value in resource.net_options %}
        {{ key }} {{ value }};
{% endfor %}
    }

{% for node in resource.nodes %}
    on {{ node.hostname }} {
        node-id {{ node.node_id }};
        address {{ node.ip }}:{{ resource.port }};
        volume 0 {
            device {{ resource.device }} minor {{ resource.minor }};
            disk {{ node.disk }};
            meta-disk internal;
        }
    }

{% endfor %}
    connection-mesh {
        hosts{% for node in resource.nodes %} {{ node.hostname }}{% endfor %};
    }
}
"#;

/// drbd-reactor promoter configuration template
const PROMOTER_TEMPLATE: &str = r#"# drbd-reactor promoter configuration
# Generated by drbd-ha

[[promoter]]
[promoter.resources.{{ promoter.resource }}]
start = [
{% if promoter.mount_strategy == "ocf" %}
    {# OCF Filesystem Agent for advanced HA scenarios - use drbd device with proper fallback #}
    "ocf:heartbeat:Filesystem {{ promoter.resource }}_fs device=/dev/drbd/by-{{ promoter.resource }}/0 directory={{ promoter.mount_point }} fstype={{ promoter.fs_type }} run_fsck=no force_unmount=true",
{% elif promoter.mount_unit %}
    {# Systemd mount unit (default) #}
    "{{ promoter.mount_unit }}",
{% endif %}
{% if promoter.vip %}
    "ocf:heartbeat:IPaddr2 {{ promoter.resource }}_vip ip={{ promoter.vip.address }} cidr_netmask={{ promoter.vip.netmask }} nic={{ promoter.vip.interface }}",
{% endif %}
{% for agent in promoter.ocf_agents %}
    "{{ agent.name }} {{ agent.instance_name }}{% for key, value in agent.params %} {{ key }}={{ value }}{% endfor %}",
{% endfor %}
{% for service in promoter.start %}
    "{{ service }}"{% if not loop.last %},{% endif %}
{% endfor %}
]
runner = "systemd"
stop-services-on-exit = {{ promoter.stop_services_on_exit }}
on-drbd-demote-failure = "{{ promoter.on_drbd_demote_failure }}"
{% if promoter.dependencies_as %}dependencies-as = "{{ promoter.dependencies_as }}"{% endif %}
{% if promoter.target_as %}target-as = "{{ promoter.target_as }}"{% endif %}
{% if promoter.on_quorum_loss %}on-quorum-loss = "{{ promoter.on_quorum_loss }}"{% endif %}
{% if promoter.preferred_nodes %}preferred-nodes = [{% for node in promoter.preferred_nodes %}"{{ node }}"{% if not loop.last %}, {% endif %}{% endfor %}]{% endif %}
{% if promoter.preferred_nodes_policy %}preferred-nodes-policy = "{{ promoter.preferred_nodes_policy }}"{% endif %}
{% if promoter.sleep_before_promote_factor %}sleep-before-promote-factor = {{ promoter.sleep_before_promote_factor }}{% endif %}
{% if promoter.mount_strategy %}# Mount strategy: {{ promoter.mount_strategy }}{% endif %}
"#;

/// Paths for DRBD configuration files
pub struct ConfigPaths;

impl ConfigPaths {
    /// DRBD resource configuration directory
    pub const DRBD_CONF_DIR: &'static str = "/etc/drbd.d";

    /// drbd-reactor configuration directory
    pub const REACTOR_CONF_DIR: &'static str = "/etc/drbd-reactor.d";

    /// Get path for DRBD resource file
    pub fn drbd_resource_path(resource_name: &str) -> String {
        format!("{}/{}.res", Self::DRBD_CONF_DIR, resource_name)
    }

    /// Get path for drbd-reactor promoter file
    pub fn promoter_path(resource_name: &str) -> String {
        format!("{}/{}.toml", Self::REACTOR_CONF_DIR, resource_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_drbd_resource() {
        let gen = ConfigGenerator::new().unwrap();

        let config = ResourceConfig {
            name: "r0".to_string(),
            port: 7789,
            minor: 0,
            device: "/dev/drbd0".to_string(),
            nodes: vec![
                NodeConfig {
                    hostname: "node1".to_string(),
                    ip: "192.168.1.1".to_string(),
                    disk: "/dev/sdb1".to_string(),
                    node_id: 0,
                },
                NodeConfig {
                    hostname: "node2".to_string(),
                    ip: "192.168.1.2".to_string(),
                    disk: "/dev/sdb1".to_string(),
                    node_id: 1,
                },
            ],
            ..Default::default()
        };

        let output = gen.generate_drbd_resource(&config).unwrap();
        assert!(output.contains("resource r0"));
        assert!(output.contains("on node1"));
        assert!(output.contains("on node2"));
        assert!(output.contains("address 192.168.1.1:7789"));
        assert!(output.contains("connection-mesh"));
        assert!(output.contains("auto-promote yes"));
    }

    #[test]
    fn test_generate_drbd_resource_no_auto_promote() {
        let gen = ConfigGenerator::new().unwrap();

        let config = ResourceConfig {
            name: "r0".to_string(),
            port: 7789,
            minor: 0,
            device: "/dev/drbd0".to_string(),
            nodes: vec![NodeConfig {
                hostname: "node1".to_string(),
                ip: "192.168.1.1".to_string(),
                disk: "/dev/sdb1".to_string(),
                node_id: 0,
            }],
            auto_promote: false, // For drbd-reactor managed resources
            ..Default::default()
        };

        let output = gen.generate_drbd_resource(&config).unwrap();
        assert!(output.contains("auto-promote no"));
        // HA resources should have additional options
        assert!(output.contains("on-suspended-primary-outdated force-secondary"));
        assert!(output.contains("on-no-data-accessible io-error"));
    }

    #[test]
    fn test_generate_promoter() {
        let gen = ConfigGenerator::new().unwrap();

        let config = PromoterPluginConfig {
            resource: "r0".to_string(),
            mount_unit: None,
            start: vec!["app.service".to_string(), "web.service".to_string()],
            stop_services_on_exit: true,
            on_drbd_demote_failure: "reboot".to_string(),
            vip: Some(VipPluginConfig {
                address: "192.168.1.100".to_string(),
                netmask: 24,
                interface: "eth0".to_string(),
            }),
            ocf_agents: vec![],
            dependencies_as: Some("Wants".to_string()),
            target_as: Some("Requires".to_string()),
            on_quorum_loss: Some("freeze".to_string()),
            preferred_nodes: Some(vec!["node1".to_string(), "node2".to_string()]),
            preferred_nodes_policy: Some("always".to_string()),
            sleep_before_promote_factor: Some(2),
            mount_strategy: Some("systemd".to_string()),
            mount_point: Some("/mnt/data".to_string()),
            fs_type: Some("xfs".to_string()),
        };

        let output = gen.generate_promoter(&config).unwrap();
        assert!(output.contains("[promoter.resources.r0]"));
        assert!(output.contains("app.service"));
        assert!(output.contains("web.service"));
        // VIP should be in OCF format inside start list (instance name, ip, cidr_netmask, nic)
        assert!(output
            .contains("ocf:heartbeat:IPaddr2 r0_vip ip=192.168.1.100 cidr_netmask=24 nic=eth0"));
        assert!(output.contains("runner = \"systemd\""));

        // Check new fields
        assert!(output.contains("dependencies-as = \"Wants\""));
        assert!(output.contains("target-as = \"Requires\""));
        assert!(output.contains("on-quorum-loss = \"freeze\""));
        assert!(output.contains("preferred-nodes = [\"node1\", \"node2\"]"));
        assert!(output.contains("preferred-nodes-policy = \"always\""));
        assert!(output.contains("sleep-before-promote-factor = 2"));
    }

    #[test]
    fn test_config_paths() {
        assert_eq!(ConfigPaths::drbd_resource_path("r0"), "/etc/drbd.d/r0.res");
        assert_eq!(
            ConfigPaths::promoter_path("r0"),
            "/etc/drbd-reactor.d/r0.toml"
        );
    }
}

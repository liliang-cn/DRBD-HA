//! Configuration file generator
//!
//! Generates DRBD resource files and drbd-reactor TOML configurations.
//!
//! This is the single source of truth for all DRBD and drbd-reactor configuration generation.
//! Other crates (drbd-utils, drbd-reactor-utils) should re-export from here.

use tera::{Context, Tera};
use thiserror::Error;

#[cfg(test)]
use std::collections::HashMap;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to load template: {0}")]
    LoadTemplate(String),
    #[error("Failed to render template: {0}")]
    Render(String),
}

pub type Result<T> = std::result::Result<T, ConfigError>;

// Re-exports for convenience
pub use types::{
    NodeConfig, OcfAgentConfig, ParamEntry, PromoterPluginConfig, ResourceConfig, VipPluginConfig,
};

// Configuration types module
mod types {
    use serde::Serialize;
    use std::collections::HashMap;

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
        pub mount_strategy: Option<String>,
        /// Mount point (required for OCF strategy)
        pub mount_point: Option<String>,
        /// Filesystem type (required for OCF strategy)
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
        pub params: Vec<ParamEntry>,
    }

    /// Ordered parameter entry (key-value pair with order preserved)
    #[derive(Debug, Clone, Serialize)]
    pub struct ParamEntry {
        pub key: String,
        pub value: String,
    }

    /// VIP configuration for drbd-reactor promoter
    #[derive(Debug, Clone, Serialize)]
    pub struct VipPluginConfig {
        pub address: String,
        pub netmask: u8,
    }
}

/// Configuration generator using Tera templates
pub struct ConfigGenerator {
    tera: Tera,
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
            device {{ resource.device }};
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
    "ocf:heartbeat:IPaddr2 vip ip={{ promoter.vip.address }} cidr_netmask={{ promoter.vip.netmask }}",
{% endif %}
{% for agent in promoter.ocf_agents %}
    "{{ agent.name }} {{ agent.instance_name }}{% for key, value in agent.params %} {{ key }}={{ value }}{% endfor %}",
{% endfor %}
{% for service in promoter.start %}
    "{{ service }}",
{% endfor %}
]
runner = "systemd"
stop-services-on-exit = {{ promoter.stop_services_on_exit }}
on-drbd-demote-failure = "{{ promoter.on_drbd_demote_failure }}"
{% if promoter.dependencies_as %}dependencies-as = "{{ promoter.dependencies_as }}"{% endif %}
{% if promoter.target_as %}target-as = "{{ promoter.target_as }}"{% endif %}
{% if promoter.on_quorum_loss %}on-quorum-loss = "{{ promoter.on_quorum_loss }}"{% endif %}
{% if promoter.preferred_nodes %}preferred-nodes = [{% for node in promoter.preferred_nodes %}"{{ node }}",{% endfor %}]{% endif %}
{% if promoter.preferred_nodes_policy %}preferred-nodes-policy = "{{ promoter.preferred_nodes_policy }}"{% endif %}
{% if promoter.sleep_before_promote_factor %}sleep-before-promote-factor = {{ promoter.sleep_before_promote_factor }}{% endif %}
{% if promoter.mount_strategy %}# Mount strategy: {{ promoter.mount_strategy }}{% endif %}
"#;

/// Paths for configuration files
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

    // ========== DRBD Resource Configuration Tests ==========

    #[test]
    fn test_config_generator_creation() {
        let gen = ConfigGenerator::new();
        assert!(
            gen.is_ok(),
            "ConfigGenerator should be created successfully"
        );
    }

    #[test]
    fn test_generate_drbd_resource_basic() {
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

        // Basic structure checks
        assert!(output.contains("resource r0"));
        assert!(output.contains("# DRBD resource configuration"));

        // Node checks
        assert!(output.contains("on node1"));
        assert!(output.contains("on node2"));
        assert!(output.contains("node-id 0"));
        assert!(output.contains("node-id 1"));

        // Address checks
        assert!(output.contains("address 192.168.1.1:7789"));
        assert!(output.contains("address 192.168.1.2:7789"));

        // Disk checks
        assert!(output.contains("disk /dev/sdb1"));

        // Device checks
        assert!(output.contains("device /dev/drbd0 minor 0"));

        // Options checks
        assert!(output.contains("connection-mesh"));
        assert!(output.contains("hosts node1 node2"));
        assert!(output.contains("auto-promote yes"));
        assert!(output.contains("quorum majority"));

        // Default options
        assert!(output.contains("on-io-error detach"));
        assert!(output.contains("protocol C"));
        assert!(output.contains("verify-alg sha256"));
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
            auto_promote: false,
            ..Default::default()
        };

        let output = gen.generate_drbd_resource(&config).unwrap();

        assert!(output.contains("auto-promote no"));
        // HA resources should have additional options
        assert!(output.contains("on-suspended-primary-outdated force-secondary"));
        assert!(output.contains("on-no-data-accessible io-error"));
    }

    #[test]
    fn test_generate_drbd_resource_custom_options() {
        let gen = ConfigGenerator::new().unwrap();

        let mut disk_options = HashMap::new();
        disk_options.insert("al-extents".to_string(), "3377".to_string());
        disk_options.insert("al-stripes".to_string(), "2".to_string());

        let mut net_options = HashMap::new();
        net_options.insert("protocol".to_string(), "A".to_string());
        net_options.insert("max-buffers".to_string(), "2048".to_string());

        let config = ResourceConfig {
            name: "mysql_data".to_string(),
            port: 7790,
            minor: 1,
            device: "/dev/drbd1".to_string(),
            nodes: vec![NodeConfig {
                hostname: "db1".to_string(),
                ip: "10.0.0.1".to_string(),
                disk: "/dev/vg01/mysql_lv".to_string(),
                node_id: 0,
            }],
            disk_options,
            net_options,
            auto_promote: true,
        };

        let output = gen.generate_drbd_resource(&config).unwrap();

        assert!(output.contains("resource mysql_data"));
        assert!(output.contains("address 10.0.0.1:7790"));
        assert!(output.contains("device /dev/drbd1 minor 1"));
        assert!(output.contains("al-extents 3377"));
        assert!(output.contains("al-stripes 2"));
        assert!(output.contains("protocol A"));
        assert!(output.contains("max-buffers 2048"));
    }

    #[test]
    fn test_generate_drbd_resource_three_nodes() {
        let gen = ConfigGenerator::new().unwrap();

        let config = ResourceConfig {
            name: "web_data".to_string(),
            port: 7800,
            minor: 5,
            device: "/dev/drbd5".to_string(),
            nodes: vec![
                NodeConfig {
                    hostname: "web1".to_string(),
                    ip: "10.0.0.1".to_string(),
                    disk: "/dev/sda1".to_string(),
                    node_id: 0,
                },
                NodeConfig {
                    hostname: "web2".to_string(),
                    ip: "10.0.0.2".to_string(),
                    disk: "/dev/sda1".to_string(),
                    node_id: 1,
                },
                NodeConfig {
                    hostname: "web3".to_string(),
                    ip: "10.0.0.3".to_string(),
                    disk: "/dev/sda1".to_string(),
                    node_id: 2,
                },
            ],
            ..Default::default()
        };

        let output = gen.generate_drbd_resource(&config).unwrap();

        assert!(output.contains("resource web_data"));
        assert!(output.contains("on web1"));
        assert!(output.contains("on web2"));
        assert!(output.contains("on web3"));
        assert!(output.contains("hosts web1 web2 web3"));
        assert!(output.contains("address 10.0.0.1:7800"));
    }

    #[test]
    fn test_resource_config_default() {
        let config = ResourceConfig::default();

        assert_eq!(config.name, "");
        assert_eq!(config.port, 7789);
        assert_eq!(config.minor, 0);
        assert_eq!(config.device, "/dev/drbd0");
        assert!(config.nodes.is_empty());
        assert!(config.auto_promote);
        assert_eq!(config.disk_options.len(), 1);
        assert_eq!(config.net_options.len(), 2);
        assert!(config.disk_options.contains_key("on-io-error"));
        assert!(config.net_options.contains_key("protocol"));
    }

    #[test]
    fn test_config_paths() {
        assert_eq!(ConfigPaths::drbd_resource_path("r0"), "/etc/drbd.d/r0.res");
        assert_eq!(
            ConfigPaths::drbd_resource_path("mysql_data"),
            "/etc/drbd.d/mysql_data.res"
        );
        assert_eq!(
            ConfigPaths::drbd_resource_path("web"),
            "/etc/drbd.d/web.res"
        );

        assert_eq!(
            ConfigPaths::promoter_path("r0"),
            "/etc/drbd-reactor.d/r0.toml"
        );
        assert_eq!(
            ConfigPaths::promoter_path("mysql_ha"),
            "/etc/drbd-reactor.d/mysql_ha.toml"
        );
        assert_eq!(
            ConfigPaths::promoter_path("web_ha"),
            "/etc/drbd-reactor.d/web_ha.toml"
        );

        assert_eq!(ConfigPaths::DRBD_CONF_DIR, "/etc/drbd.d");
        assert_eq!(ConfigPaths::REACTOR_CONF_DIR, "/etc/drbd-reactor.d");
    }

    // ========== Promoter Configuration Tests ==========

    #[test]
    fn test_generate_promoter_basic() {
        let gen = ConfigGenerator::new().unwrap();

        let config = PromoterPluginConfig {
            resource: "r0".to_string(),
            mount_unit: Some("var-lib-mysql.mount".to_string()),
            start: vec!["mysqld.service".to_string()],
            stop_services_on_exit: true,
            on_drbd_demote_failure: "continue".to_string(),
            vip: None,
            ocf_agents: vec![],
            mount_strategy: None,
            mount_point: None,
            fs_type: None,
            dependencies_as: None,
            target_as: None,
            on_quorum_loss: None,
            preferred_nodes: None,
            preferred_nodes_policy: None,
            sleep_before_promote_factor: None,
        };

        let output = gen.generate_promoter(&config).unwrap();

        assert!(output.contains("[promoter.resources.r0]"));
        assert!(output.contains("var-lib-mysql.mount"));
        assert!(output.contains("mysqld.service"));
        assert!(output.contains("stop-services-on-exit = true"));
        assert!(output.contains("on-drbd-demote-failure = \"continue\""));
        assert!(output.contains("runner = \"systemd\""));
        assert!(output.contains("# drbd-reactor promoter configuration"));
    }

    #[test]
    fn test_generate_promoter_with_vip() {
        let gen = ConfigGenerator::new().unwrap();

        let config = PromoterPluginConfig {
            resource: "r0".to_string(),
            mount_unit: Some("var-lib-mysql.mount".to_string()),
            start: vec!["mysqld.service".to_string()],
            stop_services_on_exit: true,
            on_drbd_demote_failure: "continue".to_string(),
            vip: Some(VipPluginConfig {
                address: "192.168.1.100".to_string(),
                netmask: 24,
            }),
            ocf_agents: vec![],
            mount_strategy: None,
            mount_point: None,
            fs_type: None,
            dependencies_as: None,
            target_as: None,
            on_quorum_loss: None,
            preferred_nodes: None,
            preferred_nodes_policy: None,
            sleep_before_promote_factor: None,
        };

        let output = gen.generate_promoter(&config).unwrap();

        assert!(output.contains("ocf:heartbeat:IPaddr2 r0_vip ip=192.168.1.100 cidr_netmask=24"));
        assert!(output.contains("var-lib-mysql.mount"));
    }

    #[test]
    fn test_generate_promoter_with_multiple_services() {
        let gen = ConfigGenerator::new().unwrap();

        let config = PromoterPluginConfig {
            resource: "web_ha".to_string(),
            mount_unit: Some("var-www.mount".to_string()),
            start: vec![
                "nginx.service".to_string(),
                "php-fpm.service".to_string(),
                "redis.service".to_string(),
            ],
            stop_services_on_exit: false,
            on_drbd_demote_failure: "reboot".to_string(),
            vip: None,
            ocf_agents: vec![],
            mount_strategy: None,
            mount_point: None,
            fs_type: None,
            dependencies_as: None,
            target_as: None,
            on_quorum_loss: None,
            preferred_nodes: None,
            preferred_nodes_policy: None,
            sleep_before_promote_factor: None,
        };

        let output = gen.generate_promoter(&config).unwrap();

        assert!(output.contains("nginx.service"));
        assert!(output.contains("php-fpm.service"));
        assert!(output.contains("redis.service"));
        assert!(output.contains("stop-services-on-exit = false"));
    }

    #[test]
    fn test_generate_promoter_with_ocf_agents() {
        let gen = ConfigGenerator::new().unwrap();

        let mut agent_params = HashMap::new();
        agent_params.insert("op".to_string(), "start".to_string());
        agent_params.insert("interval".to_string(), "30s".to_string());

        let config = PromoterPluginConfig {
            resource: "db_ha".to_string(),
            mount_unit: Some("var-lib-postgresql.mount".to_string()),
            start: vec!["postgresql.service".to_string()],
            stop_services_on_exit: true,
            on_drbd_demote_failure: "continue".to_string(),
            vip: None,
            ocf_agents: vec![
                OcfAgentConfig {
                    name: "ocf:heartbeat:IPaddr2".to_string(),
                    instance_name: "db_ha_vip".to_string(),
                    params: {
                        let mut params = HashMap::new();
                        params.insert("ip".to_string(), "10.0.0.50".to_string());
                        params.insert("cidr_netmask".to_string(), "24".to_string());
                        params
                    },
                },
                OcfAgentConfig {
                    name: "ocf:heartbeat:mysql".to_string(),
                    instance_name: "db_ha_conn".to_string(),
                    params: agent_params,
                },
            ],
            mount_strategy: None,
            mount_point: None,
            fs_type: None,
            dependencies_as: None,
            target_as: None,
            on_quorum_loss: None,
            preferred_nodes: None,
            preferred_nodes_policy: None,
            sleep_before_promote_factor: None,
        };

        let output = gen.generate_promoter(&config).unwrap();

        // HashMap doesn't preserve order, so check for both params individually
        assert!(output.contains("ocf:heartbeat:IPaddr2 db_ha_vip"));
        assert!(output.contains("ip=10.0.0.50"));
        assert!(output.contains("cidr_netmask=24"));
        assert!(output.contains("ocf:heartbeat:mysql db_ha_conn"));
        assert!(output.contains("op=start"));
        assert!(output.contains("interval=30s"));
        assert!(output.contains("postgresql.service"));
    }

    #[test]
    fn test_generate_promoter_ocf_mount_strategy() {
        let gen = ConfigGenerator::new().unwrap();

        let config = PromoterPluginConfig {
            resource: "data_ha".to_string(),
            mount_unit: None,
            start: vec!["app.service".to_string()],
            stop_services_on_exit: true,
            on_drbd_demote_failure: "continue".to_string(),
            vip: None,
            ocf_agents: vec![],
            mount_strategy: Some("ocf".to_string()),
            mount_point: Some("/data".to_string()),
            fs_type: Some("ext4".to_string()),
            dependencies_as: None,
            target_as: None,
            on_quorum_loss: None,
            preferred_nodes: None,
            preferred_nodes_policy: None,
            sleep_before_promote_factor: None,
        };

        let output = gen.generate_promoter(&config).unwrap();

        assert!(output.contains("ocf:heartbeat:Filesystem data_ha_fs device=/dev/drbd/by-data_ha/0 directory=/data fstype=ext4 run_fsck=no force_unmount=true"));
        assert!(output.contains("# Mount strategy: ocf"));
        assert!(output.contains("app.service"));
    }

    #[test]
    fn test_generate_promoter_advanced_options() {
        let gen = ConfigGenerator::new().unwrap();

        let config = PromoterPluginConfig {
            resource: "r0".to_string(),
            mount_unit: None,
            start: vec!["app.service".to_string()],
            stop_services_on_exit: true,
            on_drbd_demote_failure: "continue".to_string(),
            vip: Some(VipPluginConfig {
                address: "192.168.1.100".to_string(),
                netmask: 24,
            }),
            ocf_agents: vec![],
            mount_strategy: Some("systemd".to_string()),
            mount_point: Some("/mnt/data".to_string()),
            fs_type: Some("xfs".to_string()),
            dependencies_as: Some("Wants".to_string()),
            target_as: Some("Requires".to_string()),
            on_quorum_loss: Some("freeze".to_string()),
            preferred_nodes: Some(vec!["node1".to_string(), "node2".to_string()]),
            preferred_nodes_policy: Some("always".to_string()),
            sleep_before_promote_factor: Some(3),
        };

        let output = gen.generate_promoter(&config).unwrap();

        // Check all advanced options
        assert!(output.contains("dependencies-as = \"Wants\""));
        assert!(output.contains("target-as = \"Requires\""));
        assert!(output.contains("on-quorum-loss = \"freeze\""));
        // Preferred nodes format varies, check for the key and values
        assert!(output.contains("preferred-nodes"));
        assert!(output.contains("\"node1\""));
        assert!(output.contains("\"node2\""));
        assert!(output.contains("preferred-nodes-policy = \"always\""));
        assert!(output.contains("sleep-before-promote-factor = 3"));
        assert!(output.contains("# Mount strategy: systemd"));
    }

    #[test]
    fn test_generate_promoter_complete() {
        let gen = ConfigGenerator::new().unwrap();

        let config = PromoterPluginConfig {
            resource: "mysql_ha".to_string(),
            mount_unit: None,
            start: vec!["app.service".to_string(), "web.service".to_string()],
            stop_services_on_exit: true,
            on_drbd_demote_failure: "reboot".to_string(),
            vip: Some(VipPluginConfig {
                address: "192.168.1.100".to_string(),
                netmask: 24,
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

        assert!(output.contains("[promoter.resources.mysql_ha]"));
        assert!(output.contains("app.service"));
        assert!(output.contains("web.service"));
        assert!(
            output.contains("ocf:heartbeat:IPaddr2 mysql_ha_vip ip=192.168.1.100 cidr_netmask=24")
        );
        assert!(output.contains("runner = \"systemd\""));

        // All advanced options
        assert!(output.contains("dependencies-as = \"Wants\""));
        assert!(output.contains("target-as = \"Requires\""));
        assert!(output.contains("on-quorum-loss = \"freeze\""));
        // Preferred nodes format varies, check for the key and values
        assert!(output.contains("preferred-nodes"));
        assert!(output.contains("\"node1\""));
        assert!(output.contains("\"node2\""));
        assert!(output.contains("preferred-nodes-policy = \"always\""));
        assert!(output.contains("sleep-before-promote-factor = 2"));
    }

    #[test]
    fn test_vip_config_different_netmasks() {
        let vip1 = VipPluginConfig {
            address: "192.168.1.1".to_string(),
            netmask: 24,
        };

        let vip2 = VipPluginConfig {
            address: "10.0.0.1".to_string(),
            netmask: 32,
        };

        assert_eq!(vip1.address, "192.168.1.1");
        assert_eq!(vip1.netmask, 24);

        assert_eq!(vip2.address, "10.0.0.1");
        assert_eq!(vip2.netmask, 32);
    }

    #[test]
    fn test_ocf_agent_config_with_params() {
        let mut params = HashMap::new();
        params.insert("op".to_string(), "monitor".to_string());
        params.insert("timeout".to_string(), "20s".to_string());
        params.insert("interval".to_string(), "5s".to_string());
        params.insert("depth".to_string(), "0".to_string());

        let agent = OcfAgentConfig {
            name: "ocf:heartbeat:IPaddr2".to_string(),
            instance_name: "r0_vip".to_string(),
            params,
        };

        assert_eq!(agent.name, "ocf:heartbeat:IPaddr2");
        assert_eq!(agent.instance_name, "r0_vip");
        assert_eq!(agent.params.len(), 4);
        assert_eq!(agent.params.get("op"), Some(&"monitor".to_string()));
        assert_eq!(agent.params.get("timeout"), Some(&"20s".to_string()));
    }

    #[test]
    fn test_empty_start_list() {
        let gen = ConfigGenerator::new().unwrap();

        let config = PromoterPluginConfig {
            resource: "test".to_string(),
            mount_unit: None,
            start: vec![],
            stop_services_on_exit: false,
            on_drbd_demote_failure: "continue".to_string(),
            vip: None,
            ocf_agents: vec![],
            mount_strategy: None,
            mount_point: None,
            fs_type: None,
            dependencies_as: None,
            target_as: None,
            on_quorum_loss: None,
            preferred_nodes: None,
            preferred_nodes_policy: None,
            sleep_before_promote_factor: None,
        };

        let output = gen.generate_promoter(&config).unwrap();

        // Should still generate valid config
        assert!(output.contains("[promoter.resources.test]"));
        assert!(output.contains("start = ["));
        assert!(output.contains("]"));
        assert!(output.contains("stop-services-on-exit = false"));
    }

    #[test]
    fn test_empty_ocf_agents() {
        let gen = ConfigGenerator::new().unwrap();

        let config = PromoterPluginConfig {
            resource: "r0".to_string(),
            mount_unit: Some("test.mount".to_string()),
            start: vec!["service.service".to_string()],
            stop_services_on_exit: true,
            on_drbd_demote_failure: "continue".to_string(),
            vip: None,
            ocf_agents: vec![], // Empty agents list
            mount_strategy: None,
            mount_point: None,
            fs_type: None,
            dependencies_as: None,
            target_as: None,
            on_quorum_loss: None,
            preferred_nodes: None,
            preferred_nodes_policy: None,
            sleep_before_promote_factor: None,
        };

        let output = gen.generate_promoter(&config).unwrap();

        assert!(output.contains("test.mount"));
        assert!(output.contains("service.service"));
        // Should not contain any OCF agent lines except VIP
        assert!(!output.contains("ocf:heartbeat:Filesystem"));
    }

    #[test]
    fn test_error_handling_invalid_template() {
        // This test verifies that ConfigError is properly defined
        let error = ConfigError::LoadTemplate("test error".to_string());
        assert_eq!(error.to_string(), "Failed to load template: test error");

        let error = ConfigError::Render("test error".to_string());
        assert_eq!(error.to_string(), "Failed to render template: test error");
    }

    #[test]
    fn test_config_generator_default() {
        let _gen = ConfigGenerator::default();
        assert!(ConfigGenerator::new().is_ok());
    }

    #[test]
    fn test_promoter_config_all_fields() {
        let mut params = HashMap::new();
        params.insert("param1".to_string(), "value1".to_string());

        let config = PromoterPluginConfig {
            resource: "test_resource".to_string(),
            mount_unit: Some("test.mount".to_string()),
            start: vec!["s1.service".to_string(), "s2.service".to_string()],
            stop_services_on_exit: false,
            on_drbd_demote_failure: "continue".to_string(),
            vip: Some(VipPluginConfig {
                address: "10.0.0.1".to_string(),
                netmask: 32,
            }),
            ocf_agents: vec![OcfAgentConfig {
                name: "ocf:heartbeat:test".to_string(),
                instance_name: "test_inst".to_string(),
                params,
            }],
            mount_strategy: Some("ocf".to_string()),
            mount_point: Some("/mnt".to_string()),
            fs_type: Some("btrfs".to_string()),
            dependencies_as: Some("Requires".to_string()),
            target_as: Some("Wants".to_string()),
            on_quorum_loss: Some("stop".to_string()),
            preferred_nodes: Some(vec!["n1".to_string(), "n2".to_string(), "n3".to_string()]),
            preferred_nodes_policy: Some("never".to_string()),
            sleep_before_promote_factor: Some(5),
        };

        assert_eq!(config.resource, "test_resource");
        assert!(config.mount_unit.is_some());
        assert_eq!(config.start.len(), 2);
        assert!(!config.stop_services_on_exit);
        assert_eq!(config.on_drbd_demote_failure, "continue");
        assert!(config.vip.is_some());
        assert_eq!(config.ocf_agents.len(), 1);
        assert!(config.mount_strategy.is_some());
        assert!(config.mount_point.is_some());
        assert!(config.fs_type.is_some());
        assert!(config.dependencies_as.is_some());
        assert!(config.target_as.is_some());
        assert!(config.on_quorum_loss.is_some());
        assert_eq!(
            config
                .preferred_nodes
                .as_ref()
                .map(|v| v.len())
                .unwrap_or(0),
            3
        );
        assert!(config.preferred_nodes_policy.is_some());
        assert_eq!(config.sleep_before_promote_factor, Some(5));
    }

    #[test]
    fn test_node_config_all_fields() {
        let node = NodeConfig {
            hostname: "testhost".to_string(),
            ip: "10.1.1.1".to_string(),
            disk: "/dev/vg01/lv01".to_string(),
            node_id: 5,
        };

        assert_eq!(node.hostname, "testhost");
        assert_eq!(node.ip, "10.1.1.1");
        assert_eq!(node.disk, "/dev/vg01/lv01");
        assert_eq!(node.node_id, 5);
    }

    #[test]
    fn test_resource_config_all_fields() {
        let mut disk_opts = HashMap::new();
        disk_opts.insert("resync-rate".to_string(), "100M".to_string());

        let mut net_opts = HashMap::new();
        net_opts.insert("protocol".to_string(), "A".to_string());
        net_opts.insert("verify-alg".to_string(), "crc32c".to_string());

        let config = ResourceConfig {
            name: "test_r".to_string(),
            port: 7000,
            minor: 10,
            device: "/dev/drbd10".to_string(),
            nodes: vec![],
            disk_options: disk_opts,
            net_options: net_opts,
            auto_promote: false,
        };

        assert_eq!(config.name, "test_r");
        assert_eq!(config.port, 7000);
        assert_eq!(config.minor, 10);
        assert_eq!(config.device, "/dev/drbd10");
        assert!(config.nodes.is_empty());
        assert_eq!(config.disk_options.len(), 1);
        assert_eq!(config.net_options.len(), 2);
        assert!(!config.auto_promote);
    }
}

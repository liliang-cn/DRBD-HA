//! Configuration file generator
//!
//! Generates DRBD resource files and drbd-reactor TOML configurations.

use crate::error::{AppError, AppResult};
use crate::models::{CreateResourceRequest, HaProfile};
pub use config_gen::{
    ConfigPaths, NodeConfig, PromoterPluginConfig, ResourceConfig, VipPluginConfig,
};

/// Configuration generator using Tera templates
pub struct ConfigGenerator {
    inner: config_gen::ConfigGenerator,
}

impl ConfigGenerator {
    /// Create a new configuration generator
    pub fn new() -> AppResult<Self> {
        let inner = config_gen::ConfigGenerator::new()
            .map_err(|e| AppError::Config(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Generate DRBD resource configuration file content
    pub fn generate_drbd_resource(&self, config: &ResourceConfig) -> AppResult<String> {
        self.inner
            .generate_drbd_resource(config)
            .map_err(|e| AppError::Config(e.to_string()))
    }

    /// Generate drbd-reactor promoter configuration
    pub fn generate_promoter(&self, config: &PromoterPluginConfig) -> AppResult<String> {
        self.inner
            .generate_promoter(config)
            .map_err(|e| AppError::Config(e.to_string()))
    }

    /// Generate resource config from API request
    pub fn resource_from_request(
        req: &CreateResourceRequest,
        nodes: &[(String, String, String)], // (hostname, ip, disk)
    ) -> ResourceConfig {
        let mut config = ResourceConfig {
            name: req.name.clone(),
            port: req.port,
            minor: req.minor,
            device: format!("/dev/drbd{}", req.minor),
            auto_promote: req.auto_promote,
            ..Default::default()
        };

        for (i, (hostname, ip, disk)) in nodes.iter().enumerate() {
            config.nodes.push(NodeConfig {
                hostname: hostname.clone(),
                ip: ip.clone(),
                disk: disk.clone(),
                node_id: i as u32,
            });
        }

        config
    }

    /// Generate promoter config from HA profile
    pub fn promoter_from_profile(profile: &HaProfile) -> PromoterPluginConfig {
        let vip = profile.vip.as_ref().map(|v| VipPluginConfig {
            address: v.address.clone(),
            netmask: v.netmask,
            interface: v.interface.clone(),
        });

        // Get mount unit from generated_units if available
        let mount_unit = profile.generated_units.mount_unit.clone();

        PromoterPluginConfig {
            resource: profile.resource_name.clone(),
            mount_unit,
            start: profile.promoter.services.clone(),
            stop_services_on_exit: profile.promoter.stop_on_demote,
            on_drbd_demote_failure: profile.promoter.on_demote_failure.clone(),
            vip,
        }
    }
}

impl Default for ConfigGenerator {
    fn default() -> Self {
        Self::new().expect("Failed to create config generator")
    }
}
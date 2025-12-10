#[derive(Deserialize, Debug)]
struct ReactorToml {
    promoter: Vec<PromoterSection>,
}

#[derive(Deserialize, Debug)]
struct PromoterSection {
    id: String,
    resources: std::collections::HashMap<String, ResourceSection>,
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
    #[allow(dead_code)]
    runner: Option<String>,
    // Other fields ignored for now
}

pub struct ReactorDiscovery;

use crate::models::{
    GeneratedUnits, HaProfile, HaProfileStatus, HaType, IscsiConfig, NfsConfig, NvmeOfConfig,
    PromoterSettings,
};
use anyhow::Result;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use tracing::warn;

impl ReactorDiscovery {
    pub const DEFAULT_CONFIG_DIR: &'static str = "/etc/drbd-reactor.d";

    /// Scan for profiles in the configuration directory
    pub fn scan(config_dir: Option<&str>) -> Result<Vec<HaProfile>> {
        let dir = config_dir.unwrap_or(Self::DEFAULT_CONFIG_DIR);
        let path = Path::new(dir);
        let mut discovered = Vec::new();

        if !path.exists() {
            return Ok(discovered);
        }

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map_or(false, |ext| ext == "toml") {
                let content = match fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("Failed to read {}: {}", path.display(), e);
                        continue;
                    }
                };

                // Attempt to parse
                if let Ok(toml) = toml::from_str::<ReactorToml>(&content) {
                    for promoter in toml.promoter {
                        // Infer properties
                        let mut services: Vec<String> = Vec::new();
                        let mut resource_name = String::new();
                        let mut stop_on_demote = true;
                        let mut on_demote_failure = "reboot".to_string();

                        // We assume one resource per promoter usually
                        for (res_name, res_conf) in promoter.resources {
                            resource_name = res_name;
                            if let Some(starts) = res_conf.start {
                                services = starts;
                            }
                            if let Some(stop) = res_conf.stop_services_on_exit {
                                stop_on_demote = stop;
                            }
                            if let Some(fail_action) = res_conf.on_demote_failure {
                                on_demote_failure = fail_action;
                            }
                            // Break after first resource (limitation for now)
                            break;
                        }

                        // Heuristics to detect HA Type
                        let mut ha_type = HaType::Generic;
                        let mut nfs_config = None;
                        let mut iscsi_config = None;
                        let mut nvmeof_config = None;

                        // Check services for known types
                        for svc in &services {
                            if svc.contains("nfs-server") || svc.contains("exportfs") {
                                ha_type = HaType::Nfs;
                                // Placeholder for NFS config - populated with dummy data
                                // Real data would require parsing the service unit or arguments
                                nfs_config = Some(NfsConfig {
                                    export_path: "/unknown".to_string(),
                                    allowed_networks: vec!["*".to_string()],
                                    options: "rw".to_string(),
                                });
                            } else if svc.contains("target") || svc.contains("tgtd") {
                                ha_type = HaType::Iscsi;
                                iscsi_config = Some(IscsiConfig {
                                    iqn: "unknown".to_string(),
                                    allowed_initiators: vec![],
                                });
                            } else if svc.contains("nvmet") {
                                ha_type = HaType::NvmeOf;
                                nvmeof_config = Some(NvmeOfConfig {
                                    nqn: "unknown".to_string(),
                                    allowed_nqns: vec![],
                                    fabric_type: "tcp".to_string(),
                                    trsvcid: "4420".to_string(),
                                });
                            }
                        }

                        let profile = HaProfile {
                            id: uuid::Uuid::new_v4().to_string(),
                            name: promoter.id,
                            ha_type,
                            resource_name,
                            mount_point: "".to_string(), // Cannot determine reliably
                            fs_type: "xfs".to_string(),  // Default guess
                            vip: None,                   // Hard to parse from systemd override args
                            promoter: PromoterSettings {
                                services,
                                stop_on_demote,
                                on_demote_failure,
                            },
                            status: HaProfileStatus::Unknown,
                            generated_units: GeneratedUnits::default(),
                            nfs: nfs_config,
                            iscsi: iscsi_config,
                            nvmeof: nvmeof_config,
                        };
                        discovered.push(profile);
                    }
                } else {
                    warn!("Failed to parse TOML structure in {}", path.display());
                }
            }
        }
        Ok(discovered)
    }
}

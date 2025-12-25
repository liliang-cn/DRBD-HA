//! Core business logic modules

pub mod cluster_sync;
pub mod discovery;
pub mod drbd_cmd;
pub mod lvm_config;
pub mod lvm_utils;
pub mod metrics;
pub mod mount_unit;
pub mod safety;
pub mod service_init;
pub mod service_override;
pub mod shell_cmd;
pub mod ssh_manager;
pub mod store;
pub mod storage;
pub mod systemd_ctrl;
pub mod transaction;
pub mod validator;
pub mod zfs_utils;

pub use cluster_sync::{ClusterSync, HaSyncConfig};
pub use discovery::ReactorDiscovery;
pub use lvm_config::configure_lvm_filter;
pub use lvm_utils::{get_vg_info, list_lvs, list_vg_info};
pub use mount_unit::{MountUnitGenerator, MountUnitInfo};
pub use safety::SafetyChecker;
pub use service_init::ServiceInitFactory;
pub use service_override::{ServiceOverrideGenerator, ServiceOverrideInfo};
pub use shell_cmd::{run_shell_command, CommandOutput};
pub use ssh_manager::{SshCredential, SshManager};
pub use store::NodeStore;
pub use storage::{LvmProvider, StorageProvider, ZfsProvider};
pub use zfs_utils::{check_zpool as check_zpool_local, ZfsClient as ZfsUtilsClient};

// Re-export from external crates
pub use drbd_utils::{ConfigGenerator as DrbdConfigGenerator, ConfigPaths as DrbdConfigPaths, NodeConfig, ResourceConfig};
pub use drbd_reactor_utils::{ConfigGenerator as ReactorConfigGenerator, ConfigPaths as ReactorConfigPaths, OcfAgentConfig, PromoterConfig as PromoterPluginConfig, VipConfig as VipPluginConfig};

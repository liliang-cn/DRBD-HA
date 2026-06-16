//! Shared dispatch SSH configuration for the DRBD-HA workspace.
//!
//! Every executor in the workspace talks to nodes through the `dispatch` crate
//! with the same connection policy: skip ssh/known_hosts config files
//! (`/dev/null`), accept any host key (nodes get re-imaged) and a short connect
//! timeout. This module is the single source of truth for that policy so the
//! config block is not copy-pasted across crates.

use std::path::PathBuf;
use std::sync::RwLock;
use std::time::Duration;

/// Default connect timeout for node SSH operations.
pub const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 5;

/// Build the dispatch [`Config`](dispatch::Config) used throughout DRBD-HA.
///
/// `sudo` is left at its default (`false`); callers that need per-command sudo
/// either set the field on the returned value or bake `sudo` into the command
/// string themselves.
pub fn default_dispatch_config() -> dispatch::Config {
    dispatch::Config {
        ssh_config_path: Some(PathBuf::from("/dev/null")),
        config_path: Some(PathBuf::from("/dev/null")),
        host_key_checking: dispatch::HostKeyChecking::AcceptAny,
        known_hosts_file: Some(PathBuf::from("/dev/null")),
        connect_timeout: Some(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS)),
        ..Default::default()
    }
}

/// Where workspace command-runners proxy their commands to when the controller
/// is not the local host. This is a process-wide singleton (one controller
/// target per running backend), previously carried via `DRBD_HA_REMOTE_EXEC_*`
/// environment variables. Storing it behind an `RwLock` makes reconfiguration
/// atomic — a node-add reconfiguring the target can no longer be observed as a
/// torn (new host / old port) read by a concurrent command — and avoids
/// `std::env::set_var`, which is `unsafe` under the 2024 edition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandProxy {
    pub host: String,
    pub port: u16,
    pub user: String,
}

static COMMAND_PROXY: RwLock<Option<CommandProxy>> = RwLock::new(None);

/// Set (or clear, with `None`) the process-wide command proxy target.
pub fn set_command_proxy(proxy: Option<CommandProxy>) {
    *COMMAND_PROXY.write().expect("command proxy lock poisoned") = proxy;
}

/// Read the current process-wide command proxy target, if any.
pub fn command_proxy() -> Option<CommandProxy> {
    COMMAND_PROXY
        .read()
        .expect("command proxy lock poisoned")
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_has_expected_policy() {
        let cfg = default_dispatch_config();
        assert_eq!(cfg.ssh_config_path, Some(PathBuf::from("/dev/null")));
        assert_eq!(cfg.config_path, Some(PathBuf::from("/dev/null")));
        assert_eq!(cfg.known_hosts_file, Some(PathBuf::from("/dev/null")));
        assert!(matches!(
            cfg.host_key_checking,
            dispatch::HostKeyChecking::AcceptAny
        ));
        assert_eq!(cfg.connect_timeout, Some(Duration::from_secs(5)));
        assert!(!cfg.sudo);
    }
}

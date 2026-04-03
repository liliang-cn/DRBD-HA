use serde::{Deserialize, Serialize};
use std::time::Duration;

/// SSH connection configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SshConfig {
    /// Connection timeout in seconds
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout_secs: u64,

    /// Command execution timeout in seconds
    #[serde(default = "default_command_timeout")]
    pub command_timeout_secs: u64,

    /// Maximum connections per host
    #[serde(default = "default_max_connections")]
    pub max_connections_per_host: usize,

    /// Default SSH port
    #[serde(default = "default_ssh_port")]
    pub default_port: u16,

    /// Default SSH username
    #[serde(default = "default_ssh_user")]
    pub default_user: String,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            connection_timeout_secs: default_connection_timeout(),
            command_timeout_secs: default_command_timeout(),
            max_connections_per_host: default_max_connections(),
            default_port: default_ssh_port(),
            default_user: default_ssh_user(),
        }
    }
}

impl SshConfig {
    pub fn connection_timeout(&self) -> Duration {
        Duration::from_secs(self.connection_timeout_secs)
    }

    pub fn command_timeout(&self) -> Duration {
        Duration::from_secs(self.command_timeout_secs)
    }
}

fn default_connection_timeout() -> u64 {
    30
}

fn default_command_timeout() -> u64 {
    60
}

fn default_max_connections() -> usize {
    5
}

fn default_ssh_port() -> u16 {
    22
}

fn default_ssh_user() -> String {
    std::env::var("DRBD_HA_SSH_USER")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("SUDO_USER")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| {
            std::env::var("USER")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| {
            std::env::var("USERNAME")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| "root".to_string())
}

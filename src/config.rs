//! Application configuration module
//!
//! Handles loading and parsing of configuration from TOML files.

use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

use crate::error::{AppError, AppResult};

/// Main application configuration
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,

    /// SSH configuration
    #[serde(default)]
    pub ssh: SshConfig,

    /// DRBD configuration
    #[serde(default)]
    pub drbd: DrbdConfig,

    /// Logging configuration
    #[serde(default)]
    pub log: LogConfig,

    /// Database configuration
    #[serde(default)]
    pub database: DatabaseConfig,

    /// Authentication configuration
    #[serde(default)]
    pub auth: AuthConfig,
}

/// HTTP server configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Host to bind to
    #[serde(default = "default_host")]
    pub host: String,

    /// Port to listen on
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    3373
}

/// SSH connection configuration
#[derive(Debug, Clone, Deserialize)]
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
    "root".to_string()
}

/// DRBD-related paths configuration
#[derive(Debug, Clone, Deserialize)]
pub struct DrbdConfig {
    /// Path to DRBD resource configuration directory
    #[serde(default = "default_drbd_config_path")]
    pub config_path: String,

    /// Path to drbd-reactor configuration directory
    #[serde(default = "default_reactor_config_path")]
    pub reactor_config_path: String,

    /// Path to systemd unit files directory
    #[serde(default = "default_systemd_unit_path")]
    pub systemd_unit_path: String,
}

impl Default for DrbdConfig {
    fn default() -> Self {
        Self {
            config_path: default_drbd_config_path(),
            reactor_config_path: default_reactor_config_path(),
            systemd_unit_path: default_systemd_unit_path(),
        }
    }
}

fn default_drbd_config_path() -> String {
    "/etc/drbd.d".to_string()
}

fn default_reactor_config_path() -> String {
    "/etc/drbd-reactor.d".to_string()
}

fn default_systemd_unit_path() -> String {
    "/etc/systemd/system".to_string()
}

/// Logging configuration
#[derive(Debug, Clone, Deserialize)]
pub struct LogConfig {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub level: String,
    
    /// Path to log file (if not set, logs to stdout only)
    #[serde(default = "default_log_file")]
    pub file: Option<String>,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: default_log_file(),
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_file() -> Option<String> {
    None
}

/// Database configuration
#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// Path to SQLite database file
    #[serde(default = "default_db_path")]
    pub path: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: default_db_path(),
        }
    }
}

fn default_db_path() -> String {
    "/var/lib/drbd-ha/data.db".to_string()
}

/// Authentication configuration
#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    /// API token for authentication (if empty, auth is disabled)
    #[serde(default)]
    pub token: Option<String>,

    /// Whether authentication is enabled
    #[serde(default)]
    pub enabled: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            token: None,
            enabled: false,
        }
    }
}

impl AuthConfig {
    /// Check if authentication is required
    pub fn is_required(&self) -> bool {
        self.enabled && self.token.is_some()
    }

    /// Validate a token
    pub fn validate_token(&self, token: &str) -> bool {
        if !self.is_required() {
            return true;
        }
        self.token.as_ref().map(|t| t == token).unwrap_or(false)
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            ssh: SshConfig::default(),
            drbd: DrbdConfig::default(),
            log: LogConfig::default(),
            database: DatabaseConfig::default(),
            auth: AuthConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> AppResult<Self> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|e| {
            AppError::Config(format!(
                "Failed to read config file '{}': {}",
                path.as_ref().display(),
                e
            ))
        })?;

        Self::from_str(&content)
    }

    /// Load configuration from a TOML string
    pub fn from_str(content: &str) -> AppResult<Self> {
        toml::from_str(content).map_err(|e| AppError::Config(format!("Failed to parse config: {}", e)))
    }

    /// Load configuration with fallback to default
    pub fn load() -> Self {
        // 1. Try to load from DRBD_HA_CONFIG environment variable
        if let Ok(env_config_path) = std::env::var("DRBD_HA_CONFIG") {
            tracing::info!("Attempting to load config from DRBD_HA_CONFIG='{}'", env_config_path);
            match Self::from_file(&env_config_path) {
                Ok(config) => {
                    tracing::info!("Loaded configuration from environment variable: {}", env_config_path);
                    return config;
                }
                Err(e) => {
                    tracing::warn!("Failed to load config from environment variable ({}): {}", env_config_path, e);
                    // Fall through to default paths if env var fails
                }
            }
        }

        // 2. Fallback to default search paths
        let config_paths = [
            "config/default.toml",
            "/etc/drbd-ha/config.toml",
        ];

        for path in config_paths {
            if Path::new(path).exists() {
                match Self::from_file(path) {
                    Ok(config) => {
                        tracing::info!("Loaded configuration from {}", path);
                        return config;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load config from {}: {}", path, e);
                    }
                }
            }
        }

        tracing::info!("Using default configuration");
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.server.port, 3373);
        assert_eq!(config.ssh.connection_timeout_secs, 30);
        assert_eq!(config.drbd.config_path, "/etc/drbd.d");
        assert_eq!(config.database.path, "/var/lib/drbd-ha/data.db");
        assert!(!config.auth.enabled);
    }

    #[test]
    fn test_parse_config() {
        let toml = r#"
[server]
host = "127.0.0.1"
port = 8080

[ssh]
connection_timeout_secs = 60
command_timeout_secs = 120

[drbd]
config_path = "/custom/drbd.d"
"#;

        let config = AppConfig::from_str(toml).unwrap();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.ssh.connection_timeout_secs, 60);
        assert_eq!(config.drbd.config_path, "/custom/drbd.d");
    }

    #[test]
    fn test_partial_config() {
        let toml = r#"
[server]
port = 9000
"#;

        let config = AppConfig::from_str(toml).unwrap();
        assert_eq!(config.server.port, 9000);
        // Other values should be defaults
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.ssh.connection_timeout_secs, 30);
    }
}

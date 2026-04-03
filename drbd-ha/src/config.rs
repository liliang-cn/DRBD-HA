//! Application configuration module
//!
//! Handles loading and parsing of configuration from TOML files.

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
pub use ssh_cmd::config::SshConfig;

/// Main application configuration
#[derive(Debug, Clone, Deserialize, Default)]
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

    /// Controller execution mode
    #[serde(default)]
    pub controller: ControllerConfig,

    /// Logging configuration
    #[serde(default)]
    pub log: LogConfig,

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

/// Controller deployment mode
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ControllerMode {
    /// The controller itself is also a managed cluster node
    #[default]
    Embedded,
    /// The controller runs outside the cluster and proxies shell/file access over SSH
    External,
}

impl ControllerMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::External => "external",
        }
    }
}

/// Controller runtime configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ControllerConfig {
    /// Whether the controller is embedded in the cluster or external to it
    #[serde(default)]
    pub mode: ControllerMode,

    /// Optional pinned node hostname/IP used for controller-scoped shell/file operations in external mode
    #[serde(default)]
    pub proxy_host: Option<String>,

    /// SSH port for the pinned controller node (defaults to ssh.default_port when unset)
    #[serde(default)]
    pub proxy_port: Option<u16>,

    /// SSH user for the pinned controller node (defaults to ssh.default_user when unset)
    #[serde(default)]
    pub proxy_user: Option<String>,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            mode: ControllerMode::External,
            proxy_host: None,
            proxy_port: None,
            proxy_user: None,
        }
    }
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

fn platform_config_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .ok()
            .map(PathBuf::from)
            .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
            .map(|path| path.join("drbd-ha"))
    } else if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .ok()
            .map(PathBuf::from)
            .map(|path| path.join("Library/Application Support/drbd-ha"))
    } else {
        std::env::var("HOME")
            .ok()
            .map(PathBuf::from)
            .map(|path| path.join(".config/drbd-ha"))
    }
}

fn default_log_file() -> Option<String> {
    let legacy_log_dir = Path::new("/var/log/drbd-ha");
    if legacy_log_dir.exists() {
        return Some(
            legacy_log_dir
                .join("drbd-ha.log")
                .to_string_lossy()
                .to_string(),
        );
    }

    platform_config_dir().map(|dir| dir.join("drbd-ha.log").to_string_lossy().to_string())
}

/// Authentication configuration
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AuthConfig {
    /// API token for authentication (if empty, auth is disabled)
    #[serde(default)]
    pub token: Option<String>,

    /// Whether authentication is enabled
    #[serde(default)]
    pub enabled: bool,
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
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(content: &str) -> AppResult<Self> {
        toml::from_str(content)
            .map_err(|e| AppError::Config(format!("Failed to parse config: {}", e)))
    }

    /// Load configuration with fallback to default
    pub fn load() -> Self {
        // 1. Try to load from DRBD_HA_CONFIG environment variable
        if let Ok(env_config_path) = std::env::var("DRBD_HA_CONFIG") {
            tracing::info!(
                "Attempting to load config from DRBD_HA_CONFIG='{}'",
                env_config_path
            );
            match Self::from_file(&env_config_path) {
                Ok(config) => {
                    let mut config = config;
                    normalize_controller_runtime(&mut config);
                    tracing::info!(
                        "Loaded configuration from environment variable: {}",
                        env_config_path
                    );
                    return config;
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to load config from environment variable ({}): {}",
                        env_config_path,
                        e
                    );
                    // Fall through to default paths if env var fails
                }
            }
        }

        // 2. Fallback to default search paths
        let mut config_paths = vec!["config/default.toml".to_string()];
        if let Some(dir) = platform_config_dir() {
            config_paths.push(dir.join("config.toml").to_string_lossy().to_string());
        }
        config_paths.push("/etc/drbd-ha/config.toml".to_string());

        for path in config_paths {
            if Path::new(&path).exists() {
                match Self::from_file(&path) {
                    Ok(config) => {
                        let mut config = config;
                        normalize_controller_runtime(&mut config);
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
        let mut config = Self::default();
        normalize_controller_runtime(&mut config);
        config
    }
}

pub fn detected_controller_platform() -> &'static str {
    std::env::consts::OS
}

fn validate_controller_mode_for_platform(mode: &ControllerMode, platform: &str) -> AppResult<()> {
    if platform != "linux" && *mode == ControllerMode::Embedded {
        return Err(AppError::Config(format!(
            "controller.mode = '{}' requires Linux, but detected '{}'. Use controller.mode = 'external' on non-Linux hosts.",
            mode.as_str(),
            platform
        )));
    }

    Ok(())
}

fn normalize_controller_mode_for_platform(mode: &mut ControllerMode, platform: &str) -> bool {
    if platform != "linux" && *mode == ControllerMode::Embedded {
        *mode = ControllerMode::External;
        return true;
    }

    false
}

pub fn normalize_controller_runtime(config: &mut AppConfig) -> bool {
    let platform = detected_controller_platform();
    let changed = normalize_controller_mode_for_platform(&mut config.controller.mode, platform);

    if changed {
        tracing::warn!(
            "Detected non-Linux controller platform '{}'; forcing controller.mode='external' for SSH-only management",
            platform
        );
    }

    changed
}

pub fn validate_controller_runtime(config: &AppConfig) -> AppResult<()> {
    validate_controller_mode_for_platform(&config.controller.mode, detected_controller_platform())
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
        assert_eq!(config.controller.mode, ControllerMode::External);
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

    #[test]
    fn test_embedded_mode_rejected_on_non_linux() {
        let err = validate_controller_mode_for_platform(&ControllerMode::Embedded, "windows")
            .unwrap_err();
        assert!(err.to_string().contains("Use controller.mode = 'external'"));
    }

    #[test]
    fn test_external_mode_allowed_on_non_linux() {
        validate_controller_mode_for_platform(&ControllerMode::External, "windows").unwrap();
        validate_controller_mode_for_platform(&ControllerMode::External, "macos").unwrap();
    }

    #[test]
    fn test_embedded_mode_allowed_on_linux() {
        validate_controller_mode_for_platform(&ControllerMode::Embedded, "linux").unwrap();
    }

    #[test]
    fn test_normalize_embedded_mode_to_external_on_non_linux() {
        let mut mode = ControllerMode::Embedded;
        assert!(normalize_controller_mode_for_platform(&mut mode, "windows"));
        assert_eq!(mode, ControllerMode::External);
    }

    #[test]
    fn test_normalize_keeps_linux_embedded_mode() {
        let mut mode = ControllerMode::Embedded;
        assert!(!normalize_controller_mode_for_platform(&mut mode, "linux"));
        assert_eq!(mode, ControllerMode::Embedded);
    }
}

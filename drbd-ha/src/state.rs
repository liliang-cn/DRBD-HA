//! Application state management

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::config::{AppConfig, ControllerMode};
use crate::core::{CommandProxyConfig, NodeStore, SshCredential, SshManager, configure_command_proxy};
use crate::error::AppError;
use crate::error::AppResult;
use crate::models::{Node, NodeStatus};

/// Event broadcast channel capacity
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Events that can be broadcast to SSE clients
#[derive(Debug, Clone)]
pub enum BroadcastEvent {
    /// Operation progress update
    Progress {
        operation_id: String,
        operation: String,
        resource: Option<String>,
        progress: u8,
        message: String,
        completed: bool,
        success: Option<bool>,
    },
    /// System notification
    Notification {
        level: NotificationLevel,
        title: String,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
    Success,
}

/// Shared application state
pub struct AppState {
    /// Application configuration
    pub config: AppConfig,
    /// SSH connection manager
    pub ssh_manager: Arc<SshManager>,
    /// Node persistence
    pub node_store: NodeStore,
    /// SSH credentials (stored separately for security, in memory only)
    pub credentials: Arc<RwLock<HashMap<String, SshCredential>>>,
    /// Event broadcast sender for SSE
    pub event_tx: broadcast::Sender<BroadcastEvent>,
}

impl AppState {
    /// Create new application state with the given configuration
    pub fn new(config: AppConfig) -> Self {
        Self::new_with_store(config, None)
    }

    /// Create new application state with custom store (useful for tests)
    pub fn new_with_store(config: AppConfig, store_path: Option<String>) -> Self {
        let ssh_manager = Arc::new(SshManager::new(config.ssh.clone()));
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let node_store = NodeStore::new(store_path);

        Self {
            config,
            ssh_manager,
            node_store,
            credentials: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Create and initialize application state according to controller mode
    pub async fn initialize(config: AppConfig) -> AppResult<Self> {
        match config.controller.mode {
            ControllerMode::Embedded => {
                configure_command_proxy(None);
                Self::with_local_node(config).await
            }
            ControllerMode::External => {
                let state = Self::new(config);
                state.refresh_command_proxy();
                Ok(state)
            }
        }
    }

    pub fn is_external_controller(&self) -> bool {
        self.config.controller.mode == ControllerMode::External
    }

    fn configured_controller_target(&self) -> Option<CommandProxyConfig> {
        let host = self.config.controller.proxy_host.clone()?;
        let port = self
            .config
            .controller
            .proxy_port
            .unwrap_or(self.config.ssh.default_port);
        let user = self
            .config
            .controller
            .proxy_user
            .clone()
            .unwrap_or_else(|| self.config.ssh.default_user.clone());

        Some(CommandProxyConfig { host, port, user })
    }

    fn auto_controller_target(&self) -> Option<CommandProxyConfig> {
        let nodes = self.node_store.get_all().ok()?;
        let selected = nodes
            .iter()
            .find(|node| node.status == NodeStatus::Online)
            .or_else(|| nodes.first())?;

        Some(CommandProxyConfig {
            host: selected.ip.clone(),
            port: selected.ssh_port,
            user: selected.ssh_user.clone(),
        })
    }

    fn controller_target_config(&self) -> Option<CommandProxyConfig> {
        if !self.is_external_controller() {
            return None;
        }

        self.configured_controller_target()
            .or_else(|| self.auto_controller_target())
    }

    pub fn refresh_command_proxy(&self) {
        if self.is_external_controller() {
            configure_command_proxy(self.controller_target_config());
        } else {
            configure_command_proxy(None);
        }
    }

    pub fn controller_hostname(&self) -> String {
        if self.is_external_controller() {
            if let Some(proxy_host) = self.config.controller.proxy_host.clone() {
                if let Ok(nodes) = self.node_store.get_all() {
                    if let Some(node) = nodes.iter().find(|node| {
                        node.hostname == proxy_host || node.ip == proxy_host || node.id == proxy_host
                    }) {
                        return node.hostname.clone();
                    }
                }
                return proxy_host;
            }

            if let Ok(nodes) = self.node_store.get_all() {
                if let Some(node) = nodes
                    .iter()
                    .find(|node| node.status == NodeStatus::Online)
                    .or_else(|| nodes.first())
                {
                    return node.hostname.clone();
                }
            }

            return "external-controller".to_string();
        }

        gethostname::gethostname().to_string_lossy().to_string()
    }

    pub fn is_controller_node(&self, node: &Node) -> bool {
        if node.is_local {
            return true;
        }

        self.matches_controller_target(&node.hostname, &node.ip, &node.id)
    }

    pub fn matches_controller_target(&self, hostname: &str, ip: &str, id: &str) -> bool {
        if !self.is_external_controller() {
            return false;
        }

        if let Some(proxy_host) = self.config.controller.proxy_host.as_deref() {
            return hostname == proxy_host || ip == proxy_host || id == proxy_host;
        }

        if let Ok(nodes) = self.node_store.get_all() {
            if let Some(node) = nodes
                .iter()
                .find(|node| node.status == NodeStatus::Online)
                .or_else(|| nodes.first())
            {
                return node.hostname == hostname || node.ip == ip || node.id == id;
            }
        }

        false
    }

    fn controller_target(&self) -> AppResult<(String, u16, String, SshCredential)> {
        let target = self.controller_target_config().ok_or_else(|| {
            AppError::Config(
                "No managed nodes available for external controller mode. Add at least one SSH-reachable node first."
                    .to_string(),
            )
        })?;
        Ok((
            target.host,
            target.port,
            target.user,
            SshCredential::Password("ignored".to_string()),
        ))
    }

    fn shell_escape_single_quotes(value: &str) -> String {
        value.replace('\'', "'\"'\"'")
    }

    pub async fn read_controller_file(&self, path: &str) -> AppResult<String> {
        if self.is_external_controller() {
            let (host, port, user, credential) = self.controller_target()?;
            self.ssh_manager
                .read_file(&host, port, &user, &credential, path)
                .await
        } else {
            tokio::fs::read_to_string(path)
                .await
                .map_err(|e| AppError::Config(format!("Failed to read file '{}': {}", path, e)))
        }
    }

    pub async fn write_controller_file(&self, path: &str, content: &str) -> AppResult<()> {
        if self.is_external_controller() {
            let (host, port, user, credential) = self.controller_target()?;
            self.ssh_manager
                .write_file(&host, port, &user, &credential, path, content)
                .await
        } else {
            if let Some(parent) = std::path::Path::new(path).parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    AppError::Config(format!(
                        "Failed to create parent directory for '{}': {}",
                        path, e
                    ))
                })?;
            }
            tokio::fs::write(path, content)
                .await
                .map_err(|e| AppError::Config(format!("Failed to write file '{}': {}", path, e)))
        }
    }

    pub async fn create_controller_dir_all(&self, path: &str) -> AppResult<()> {
        if self.is_external_controller() {
            let (host, port, user, credential) = self.controller_target()?;
            let escaped_path = Self::shell_escape_single_quotes(path);
            let cmd = format!("mkdir -p '{}'", escaped_path);
            let output = self
                .ssh_manager
                .execute(&host, port, &user, &credential, &cmd)
                .await?;

            if !output.success() {
                return Err(AppError::Internal(format!(
                    "Failed to create controller directory '{}': {}",
                    path, output.stderr
                )));
            }
            Ok(())
        } else {
            tokio::fs::create_dir_all(path).await.map_err(|e| {
                AppError::Config(format!(
                    "Failed to create controller directory '{}': {}",
                    path, e
                ))
            })
        }
    }

    pub async fn controller_file_exists(&self, path: &str) -> AppResult<bool> {
        if self.is_external_controller() {
            let (host, port, user, credential) = self.controller_target()?;
            self.ssh_manager
                .file_exists(&host, port, &user, &credential, path)
                .await
        } else {
            Ok(std::path::Path::new(path).exists())
        }
    }

    pub async fn remove_controller_file(&self, path: &str) -> AppResult<()> {
        if self.is_external_controller() {
            let (host, port, user, credential) = self.controller_target()?;
            let escaped_path = Self::shell_escape_single_quotes(path);
            let cmd = format!("rm -f '{}'", escaped_path);
            let output = self
                .ssh_manager
                .execute(&host, port, &user, &credential, &cmd)
                .await?;

            if !output.success() {
                return Err(AppError::Internal(format!(
                    "Failed to remove controller file '{}': {}",
                    path, output.stderr
                )));
            }
            Ok(())
        } else {
            match tokio::fs::remove_file(path).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(AppError::Config(format!(
                    "Failed to remove controller file '{}': {}",
                    path, e
                ))),
            }
        }
    }

    pub async fn rename_controller_file(&self, from: &str, to: &str) -> AppResult<()> {
        if self.is_external_controller() {
            let (host, port, user, credential) = self.controller_target()?;
            let escaped_from = Self::shell_escape_single_quotes(from);
            let escaped_to = Self::shell_escape_single_quotes(to);
            let cmd = format!("mv '{}' '{}'", escaped_from, escaped_to);
            let output = self
                .ssh_manager
                .execute(&host, port, &user, &credential, &cmd)
                .await?;

            if !output.success() {
                return Err(AppError::Internal(format!(
                    "Failed to rename controller file '{}' to '{}': {}",
                    from, to, output.stderr
                )));
            }
            Ok(())
        } else {
            tokio::fs::rename(from, to).await.map_err(|e| {
                AppError::Config(format!(
                    "Failed to rename controller file '{}' to '{}': {}",
                    from, to, e
                ))
            })
        }
    }

    pub async fn list_controller_dir_entries(&self, dir: &str) -> AppResult<Vec<String>> {
        if self.is_external_controller() {
            let (host, port, user, credential) = self.controller_target()?;
            let escaped_dir = Self::shell_escape_single_quotes(dir);
            let cmd = format!("find '{}' -maxdepth 1 -type f -printf '%f\n'", escaped_dir);
            let output = self
                .ssh_manager
                .execute(&host, port, &user, &credential, &cmd)
                .await?;

            if !output.success() {
                return Err(AppError::Internal(format!(
                    "Failed to list controller directory '{}': {}",
                    dir, output.stderr
                )));
            }

            Ok(output
                .stdout
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(|line| line.to_string())
                .collect())
        } else {
            let mut entries = tokio::fs::read_dir(dir).await.map_err(|e| {
                AppError::Config(format!("Failed to read directory '{}': {}", dir, e))
            })?;
            let mut file_names = Vec::new();
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Some(name) = entry.file_name().to_str() {
                    file_names.push(name.to_string());
                }
            }
            Ok(file_names)
        }
    }

    /// Subscribe to broadcast events
    pub fn subscribe_events(&self) -> broadcast::Receiver<BroadcastEvent> {
        self.event_tx.subscribe()
    }

    /// Broadcast a progress event
    #[allow(clippy::too_many_arguments)]
    pub fn send_progress(
        &self,
        operation_id: &str,
        operation: &str,
        resource: Option<&str>,
        progress: u8,
        message: &str,
        completed: bool,
        success: Option<bool>,
    ) {
        let _ = self.event_tx.send(BroadcastEvent::Progress {
            operation_id: operation_id.to_string(),
            operation: operation.to_string(),
            resource: resource.map(|s| s.to_string()),
            progress,
            message: message.to_string(),
            completed,
            success,
        });
    }

    /// Broadcast a notification
    pub fn send_notification(&self, level: NotificationLevel, title: &str, message: &str) {
        let _ = self.event_tx.send(BroadcastEvent::Notification {
            level,
            title: title.to_string(),
            message: message.to_string(),
        });
    }

    /// Initialize with local node
    pub async fn with_local_node(config: AppConfig) -> AppResult<Self> {
        tracing::debug!("with_local_node: Creating AppState struct");
        let state = Self::new(config);
        tracing::debug!("with_local_node: AppState created");

        // Check if local node exists, if not create it
        tracing::debug!("with_local_node: Checking for local node");
        if state.node_store.get("local")?.is_none() {
            tracing::info!("with_local_node: Local node not found, creating...");
            let hostname = gethostname::gethostname().to_string_lossy().to_string();

            // Get the local IP address (non-loopback)
            tracing::debug!("with_local_node: Detecting local IP");
            let local_ip = Self::get_local_ip();
            tracing::debug!("with_local_node: Local IP detected: {}", local_ip);

            let local_node = Node {
                id: "local".to_string(),
                hostname: hostname.clone(),
                ip: local_ip,
                ssh_port: 22,
                ssh_user: "root".to_string(),
                is_local: true,
                status: NodeStatus::Online,
                status_message: None,
                last_seen: Some(chrono::Utc::now()),
            };

            tracing::debug!("with_local_node: Inserting local node");
            state.node_store.insert(&local_node)?;
            tracing::debug!("with_local_node: Local node inserted");
        }

        Ok(state)
    }

    /// Get the local non-loopback IP address
    pub fn get_local_ip() -> String {
        // Try to get the local IP address using local-ip-address crate
        if let Ok(ip) = local_ip_address::local_ip() {
            return ip.to_string();
        }

        // Fallback: try to get IP from hostname command
        if let Ok(output) = std::process::Command::new("hostname").arg("-I").output() {
            if output.status.success() {
                let ips = String::from_utf8_lossy(&output.stdout);
                // Take the first IP address
                if let Some(ip) = ips.split_whitespace().next() {
                    if !ip.is_empty() {
                        return ip.to_string();
                    }
                }
            }
        }

        // Last resort fallback
        tracing::warn!("Could not detect local IP address, using 127.0.0.1");
        "127.0.0.1".to_string()
    }

    /// Get DRBD resource configuration file path
    pub fn drbd_resource_path(&self, resource_name: &str) -> String {
        format!("{}/{}.res", self.config.drbd.config_path, resource_name)
    }

    /// Get DRBD configuration directory path
    pub fn drbd_config_dir(&self) -> &str {
        &self.config.drbd.config_path
    }

    /// Get drbd-reactor promoter configuration file path
    pub fn reactor_config_path(&self, profile_name: &str) -> String {
        format!(
            "{}/{}.toml",
            self.config.drbd.reactor_config_path, profile_name
        )
    }

    /// Get drbd-reactor configuration directory path
    pub fn reactor_config_dir(&self) -> &str {
        &self.config.drbd.reactor_config_path
    }

    /// Get systemd unit file directory path
    pub fn systemd_unit_dir(&self) -> &str {
        &self.config.drbd.systemd_unit_path
    }
}

//! Application state management

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::config::AppConfig;
use crate::core::{NodeStore, SshCredential, SshManager};
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
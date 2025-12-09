//! Cluster Configuration Sync
//!
//! Synchronizes HA configuration files to all cluster nodes.
//! This ensures mount units, service overrides, and promoter configs
//! are consistent across the cluster for proper failover.

use crate::core::{Database, SshCredential, SshManager};
use crate::error::{AppError, AppResult};
use crate::models::Node;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

/// Cluster configuration synchronizer
pub struct ClusterSync {
    ssh_manager: Arc<SshManager>,
    db: Arc<Database>,
}

/// Files to sync for HA configuration
pub struct HaSyncConfig {
    /// Mount unit file path and content
    pub mount_unit: Option<(String, String)>,
    /// Service override files (path, content)
    pub service_overrides: Vec<(String, String)>,
    /// Promoter config file path and content
    pub promoter_config: (String, String),
}

impl ClusterSync {
    /// Create a new cluster sync instance
    pub fn new(
        ssh_manager: Arc<SshManager>,
        db: Arc<Database>,
        _credentials: Arc<RwLock<HashMap<String, SshCredential>>>, // Kept for signature compatibility if needed, or remove
    ) -> Self {
        Self {
            ssh_manager,
            db,
        }
    }

    /// Get credential for a node (Dummy)
    async fn get_credential(&self, _node: &Node) -> AppResult<Option<SshCredential>> {
        // We don't use credentials anymore, just return a dummy one
        Ok(Some(SshCredential::Password("ignored".to_string())))
    }

    /// Sync HA configuration to all remote nodes
    ///
    /// This copies mount units, service overrides, and promoter configs
    /// to all nodes in the cluster (except the local node).
    pub async fn sync_ha_config(&self, config: &HaSyncConfig) -> AppResult<Vec<String>> {
        let nodes = self.db.get_all_nodes()?;
        let mut synced_nodes = Vec::new();
        let mut errors = Vec::new();

        for node in nodes {
            // Skip local node
            if node.is_local {
                continue;
            }

            // Get credential (dummy)
            let credential = match self.get_credential(&node).await? {
                Some(c) => c,
                None => continue,
            };

            // Sync files to this node
            match self
                .sync_to_node(&node, &credential, config)
                .await
            {
                Ok(_) => {
                    tracing::info!("Synced HA config to node {}", node.hostname);
                    synced_nodes.push(node.hostname.clone());
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to sync HA config to node {}: {}",
                        node.hostname,
                        e
                    );
                    errors.push(format!("{}: {}", node.hostname, e));
                }
            }
        }

        if !errors.is_empty() {
            tracing::warn!("Some nodes failed to sync: {:?}", errors);
        }

        Ok(synced_nodes)
    }

    /// Sync configuration files to a single node
    async fn sync_to_node(
        &self,
        node: &Node,
        credential: &SshCredential,
        config: &HaSyncConfig,
    ) -> AppResult<()> {
        // Sync mount unit if present
        if let Some((path, content)) = &config.mount_unit {
            self.write_remote_file(node, credential, path, content)
                .await?;
        }

        // Sync service overrides
        for (path, content) in &config.service_overrides {
            // Ensure directory exists
            let dir = std::path::Path::new(path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            if !dir.is_empty() {
                let mkdir_cmd = format!("mkdir -p '{}'", dir);
                self.ssh_manager
                    .execute(&node.ip, node.ssh_port, &node.ssh_user, credential, &mkdir_cmd)
                    .await?;
            }

            self.write_remote_file(node, credential, path, content)
                .await?;
        }

        // Sync promoter config
        let (path, content) = &config.promoter_config;
        self.write_remote_file(node, credential, path, content)
            .await?;

        // Reload systemd daemon on remote node
        let reload_cmd = "systemctl daemon-reload";
        self.ssh_manager
            .execute(&node.ip, node.ssh_port, &node.ssh_user, credential, reload_cmd)
            .await?;

        Ok(())
    }

    /// Write a file to a remote node
    async fn write_remote_file(
        &self,
        node: &Node,
        credential: &SshCredential,
        path: &str,
        content: &str,
    ) -> AppResult<()> {
        self.ssh_manager
            .write_file(&node.ip, node.ssh_port, &node.ssh_user, credential, path, content)
            .await
            .map_err(|e| AppError::Ssh(format!(
                "Failed to write {} on {}: {}",
                path, node.hostname, e
            )))
    }

    /// Remove HA configuration from all remote nodes
    pub async fn remove_ha_config(&self, config: &HaSyncConfig) -> AppResult<Vec<String>> {
        let nodes = self.db.get_all_nodes()?;
        let mut synced_nodes = Vec::new();

        for node in nodes {
            if node.is_local {
                continue;
            }

            let credential = match self.get_credential(&node).await? {
                Some(c) => c,
                None => continue,
            };

            let mut any_error = false;

            // Build remove commands
            let mut files_to_remove = Vec::new();

            if let Some((path, _)) = &config.mount_unit {
                files_to_remove.push(path.clone());
            }

            // 2. Remove service overrides
            for (path, _) in &config.service_overrides {
                // Remove the primary override file
                if let Err(e) = self
                    .ssh_manager
                    .delete_file(&node.ip, node.ssh_port, &node.ssh_user, &credential, path)
                    .await
                {
                    tracing::warn!("Failed to delete override {} on {}: {}", path, node.hostname, e);
                    any_error = true;
                }
                
                // Also remove the runtime reactor.conf if it exists
                // Path format: /etc/systemd/system/{service_name}.d/ha-override.conf
                // We want: /run/systemd/system/{service_name}.d/reactor.conf
                if let Some(parent) = std::path::Path::new(path).parent() {
                    if let Some(dir_name) = parent.file_name() {
                        let run_path = format!("/run/systemd/system/{}/reactor.conf", dir_name.to_string_lossy());
                        tracing::info!("Removing remote runtime override on {}: {}", node.hostname, run_path);
                        let _ = self
                            .ssh_manager
                            .delete_file(&node.ip, node.ssh_port, &node.ssh_user, &credential, &run_path)
                            .await;
                    }
                }
            }

            files_to_remove.push(config.promoter_config.0.clone());

            // Remove files
            for path in &files_to_remove {
                let cmd = format!("rm -f '{}'", path);
                let _ = self
                    .ssh_manager
                    .execute(&node.ip, node.ssh_port, &node.ssh_user, &credential, &cmd)
                    .await;
            }

            // Reload systemd
            let _ = self
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    "systemctl daemon-reload",
                )
                .await;

            synced_nodes.push(node.hostname.clone());
        }

        Ok(synced_nodes)
    }

    /// Remove DRBD resource from all remote nodes
    pub async fn remove_drbd_resource(&self, resource_name: &str) -> AppResult<Vec<String>> {
        let nodes = self.db.get_all_nodes()?;
        let mut synced_nodes = Vec::new();

        for node in nodes {
            if node.is_local {
                continue;
            }

            let credential = match self.get_credential(&node).await? {
                Some(c) => c,
                None => continue,
            };

            // Bring down the resource on remote node
            let down_cmd = format!("drbdadm down {} 2>/dev/null || true", resource_name);
            let _ = self
                .ssh_manager
                .execute(&node.ip, node.ssh_port, &node.ssh_user, &credential, &down_cmd)
                .await;

            // Remove config file
            let config_path = format!("/etc/drbd.d/{}.res", resource_name);
            let rm_cmd = format!("rm -f '{}'", config_path);
            let _ = self
                .ssh_manager
                .execute(&node.ip, node.ssh_port, &node.ssh_user, &credential, &rm_cmd)
                .await;

            synced_nodes.push(node.hostname.clone());
            tracing::info!("Removed DRBD resource {} from node {}", resource_name, node.hostname);
        }

        Ok(synced_nodes)
    }
}

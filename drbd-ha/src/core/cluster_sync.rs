//! Cluster Configuration Sync
//!
//! Synchronizes HA configuration files to all cluster nodes.
//! This ensures mount units, service overrides, and promoter configs
//! are consistent across the cluster for proper failover.

use crate::core::{systemd_ctrl::RemoteSystemdController, NodeStore, SshCredential, SshManager};
use crate::error::{AppError, AppResult};
use crate::models::Node;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Cluster configuration synchronizer
pub struct ClusterSync {
    ssh_manager: Arc<SshManager>,
    node_store: NodeStore,
}

/// Files to sync for HA configuration
pub struct HaSyncConfig {
    /// DRBD resource config file path and content
    pub drbd_resource_config: Option<(String, String)>,
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
        node_store: NodeStore,
        _credentials: Arc<RwLock<HashMap<String, SshCredential>>>, // Kept for signature compatibility if needed, or remove
    ) -> Self {
        Self {
            ssh_manager,
            node_store,
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
        let nodes = self.node_store.get_all()?;
        let remote_nodes: Vec<_> = nodes.iter().filter(|n| !n.is_local).collect();

        tracing::info!(
            "Starting HA config sync to {} remote node(s)",
            remote_nodes.len()
        );

        let mut synced_nodes = Vec::new();
        let mut errors = Vec::new();

        for node in nodes {
            // Skip local node
            if node.is_local {
                tracing::debug!("Skipping local node {}", node.hostname);
                continue;
            }

            tracing::info!(
                "Syncing HA config to node {} ({}:{})",
                node.hostname,
                node.ip,
                node.ssh_port
            );

            // Get credential (dummy)
            let credential = match self.get_credential(&node).await? {
                Some(c) => c,
                None => {
                    tracing::warn!("No credential available for node {}", node.hostname);
                    continue;
                }
            };

            // Sync files to this node
            match self.sync_to_node(&node, &credential, config).await {
                Ok(_) => {
                    tracing::info!("✓ Successfully synced HA config to node {}", node.hostname);
                    synced_nodes.push(node.hostname.clone());
                }
                Err(e) => {
                    tracing::error!(
                        "✗ Failed to sync HA config to node {}: {}",
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

        tracing::info!(
            "HA config sync completed: {} succeeded, {} failed",
            synced_nodes.len(),
            errors.len()
        );

        Ok(synced_nodes)
    }

    /// Sync configuration files to a single node
    async fn sync_to_node(
        &self,
        node: &Node,
        credential: &SshCredential,
        config: &HaSyncConfig,
    ) -> AppResult<()> {
        // Sync DRBD resource config first (most critical)
        if let Some((path, content)) = &config.drbd_resource_config {
            tracing::info!(
                "Syncing DRBD resource config to node {}: path={}",
                node.hostname,
                path
            );

            // Ensure directory exists for DRBD config
            let dir = std::path::Path::new(path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            if !dir.is_empty() {
                let mkdir_cmd = format!("mkdir -p '{}'", dir);
                let sudo_cmd = if node.ssh_user != "root" {
                    format!("sudo {}", mkdir_cmd)
                } else {
                    mkdir_cmd
                };
                self.ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        credential,
                        &sudo_cmd,
                    )
                    .await?;
            }

            self.write_remote_file(node, credential, path, content)
                .await?;

            // 使用 drbd-utils 验证远程节点上的DRBD配置状态
            let resource_name = std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");

            let verification_config = drbd_utils::VerificationConfig::quick(); // Quick check for cluster sync

            let node_ip = node.ip.clone();
            let node_port = node.ssh_port;
            let node_user = node.ssh_user.clone();
            let ssh_manager = self.ssh_manager.clone();
            let ssh_executor = move |cmd: String| async move {
                match ssh_manager
                    .execute(&node_ip, node_port, &node_user, credential, &cmd)
                    .await
                {
                    Ok(output) => Ok(output.stdout),
                    Err(e) => Err(shell_cmd::error::ShellError::Execution(e.to_string())),
                }
            };

            match drbd_utils::DrbdVerifier::verify_remote_drbd_status(
                resource_name,
                ssh_executor,
                verification_config,
            )
            .await
            {
                Ok(result) => {
                    if result.success {
                        tracing::info!(
                            "✓ DRBD config verified on remote node {}: {} (attempts: {})",
                            node.hostname,
                            resource_name,
                            result.attempts
                        );
                    } else {
                        tracing::warn!(
                            "⚠ DRBD verification failed on remote node {}: {}",
                            node.hostname,
                            result.details.status_info
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "⚠ Failed to verify DRBD status on remote node {}: {}",
                        node.hostname,
                        e
                    );
                }
            }

            tracing::info!("✓ DRBD resource config synced to node {}", node.hostname);
        }

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
                let sudo_cmd = if node.ssh_user != "root" {
                    format!("sudo {}", mkdir_cmd)
                } else {
                    mkdir_cmd
                };
                self.ssh_manager
                    .execute(
                        &node.ip,
                        node.ssh_port,
                        &node.ssh_user,
                        credential,
                        &sudo_cmd,
                    )
                    .await?;
            }

            self.write_remote_file(node, credential, path, content)
                .await?;
        }

        // Sync promoter config
        let (path, content) = &config.promoter_config;

        tracing::info!(
            "Syncing promoter config to node {}: path={}, content_len={}",
            node.hostname,
            path,
            content.len()
        );

        // Ensure directory exists for promoter config
        let dir = std::path::Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        if !dir.is_empty() {
            tracing::debug!("Creating directory {} on node {}", dir, node.hostname);
            let mkdir_cmd = format!("mkdir -p '{}'", dir);
            let mkdir_result = self
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    credential,
                    &mkdir_cmd,
                )
                .await;

            if let Err(e) = mkdir_result {
                tracing::error!(
                    "Failed to create directory {} on {}: {}",
                    dir,
                    node.hostname,
                    e
                );
                return Err(e);
            }
            tracing::debug!(
                "Successfully created directory {} on {}",
                dir,
                node.hostname
            );
        }

        tracing::info!(
            "Writing promoter config file {} to node {}",
            path,
            node.hostname
        );
        let write_result = self
            .write_remote_file(node, credential, path, content)
            .await;

        if let Err(e) = write_result {
            tracing::error!(
                "Failed to write promoter config {} to node {}: {}",
                path,
                node.hostname,
                e
            );
            return Err(e);
        }

        tracing::info!(
            "Successfully wrote promoter config {} to node {}",
            path,
            node.hostname
        );

        // Reload systemd daemon on remote node
        tracing::info!(
            "Reloading systemd and reloading drbd-reactor on node {}",
            node.hostname
        );
        let remote_sys = RemoteSystemdController::new(self.ssh_manager.clone());

        if let Err(e) = remote_sys
            .daemon_reload(&node.ip, node.ssh_port, &node.ssh_user, credential)
            .await
        {
            tracing::error!("Failed to daemon-reload on {}: {}", node.hostname, e);
            return Err(e);
        }

        if let Err(e) = remote_sys
            .reload(
                &node.ip,
                node.ssh_port,
                &node.ssh_user,
                credential,
                "drbd-reactor.service",
            )
            .await
        {
            tracing::error!("Failed to reload drbd-reactor on {}: {}", node.hostname, e);
            return Err(e);
        }

        tracing::info!("Successfully synced all configs to node {}", node.hostname);

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
        tracing::debug!(
            "Writing file to remote node {}: path={}, size={} bytes",
            node.hostname,
            path,
            content.len()
        );

        // Use sudo tee for non-root users to write files requiring root permissions
        let cmd = if node.ssh_user != "root" {
            // Escape single quotes in content
            let escaped_content = content.replace('\'', "'\\''");
            // Use sudo tee to write with root privileges
            format!(
                "echo '{}' | sudo tee '{}' > /dev/null",
                escaped_content, path
            )
        } else {
            // For root user, use cat with heredoc for better handling of special characters
            format!("cat > '{}' << 'EOF'\n{}\nEOF", path, content)
        };

        let result = self
            .ssh_manager
            .execute(&node.ip, node.ssh_port, &node.ssh_user, credential, &cmd)
            .await;

        match &result {
            Ok(output) => {
                if output.exit_code == 0 {
                    tracing::debug!("Successfully wrote file {} to node {}", path, node.hostname);
                } else {
                    tracing::error!(
                        "Failed to write {} on {}: {}",
                        path,
                        node.hostname,
                        output.stderr
                    );
                    return Err(AppError::Ssh(format!(
                        "Failed to write {} on {}: {}",
                        path, node.hostname, output.stderr
                    )));
                }
            }
            Err(e) => {
                tracing::error!(
                    "Failed to write {} on {} ({}:{}): {}",
                    path,
                    node.hostname,
                    node.ip,
                    node.ssh_port,
                    e
                );
                return Err(AppError::Ssh(format!(
                    "Failed to write {} on {}: {}",
                    path, node.hostname, e
                )));
            }
        }

        Ok(())
    }

    /// Remove HA configuration from all remote nodes
    pub async fn remove_ha_config(&self, config: &HaSyncConfig) -> AppResult<Vec<String>> {
        let nodes = self.node_store.get_all()?;
        let mut synced_nodes = Vec::new();

        for node in nodes {
            if node.is_local {
                continue;
            }

            let credential = match self.get_credential(&node).await? {
                Some(c) => c,
                None => continue,
            };

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
                    tracing::warn!(
                        "Failed to delete override {} on {}: {}",
                        path,
                        node.hostname,
                        e
                    );
                }

                // Also remove the runtime reactor.conf if it exists
                // Path format: /etc/systemd/system/{service_name}.d/ha-override.conf
                // We want: /run/systemd/system/{service_name}.d/reactor.conf
                if let Some(parent) = std::path::Path::new(path).parent() {
                    if let Some(dir_name) = parent.file_name() {
                        let run_path = format!(
                            "/run/systemd/system/{}/reactor.conf",
                            dir_name.to_string_lossy()
                        );
                        tracing::info!(
                            "Removing remote runtime override on {}: {}",
                            node.hostname,
                            run_path
                        );
                        let _ = self
                            .ssh_manager
                            .delete_file(
                                &node.ip,
                                node.ssh_port,
                                &node.ssh_user,
                                &credential,
                                &run_path,
                            )
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
            let remote_sys = RemoteSystemdController::new(self.ssh_manager.clone());
            let _ = remote_sys
                .daemon_reload(&node.ip, node.ssh_port, &node.ssh_user, &credential)
                .await;

            synced_nodes.push(node.hostname.clone());
        }

        Ok(synced_nodes)
    }

    /// Remove DRBD resource from all remote nodes
    pub async fn remove_drbd_resource(
        &self,
        resource_name: &str,
        config_path: &str,
    ) -> AppResult<Vec<String>> {
        let nodes = self.node_store.get_all()?;
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
            let base_cmd = crate::core::drbd_cmd::DrbdCmd::down_cmd(resource_name)?;
            let down_cmd = format!("{} 2>/dev/null || true", base_cmd);
            let _ = self
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &down_cmd,
                )
                .await;

            // Remove config file
            let config_path = format!("{}/{}.res", config_path, resource_name);
            let rm_cmd = format!("rm -f '{}'", config_path);
            let _ = self
                .ssh_manager
                .execute(
                    &node.ip,
                    node.ssh_port,
                    &node.ssh_user,
                    &credential,
                    &rm_cmd,
                )
                .await;

            synced_nodes.push(node.hostname.clone());
            tracing::info!(
                "Removed DRBD resource {} from node {}",
                resource_name,
                node.hostname
            );
        }

        Ok(synced_nodes)
    }
}

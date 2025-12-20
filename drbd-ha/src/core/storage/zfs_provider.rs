use anyhow::Result;
use async_trait::async_trait;
use tracing::{info, warn};

use crate::core::{run_shell_command, SshCredential, CommandOutput};
use std::sync::Arc;

use super::provider::StorageProvider;

/// ZFS storage provider
pub struct ZfsProvider {
    pool_name: String,
    ssh_manager: Option<Arc<crate::core::SshManager>>,
    ssh_target: Option<(String, u16, String, SshCredential)>,
}

impl ZfsProvider {
    /// Create a local ZFS provider
    pub fn new_local(pool_name: String) -> Self {
        Self {
            pool_name,
            ssh_manager: None,
            ssh_target: None,
        }
    }

    /// Create a remote ZFS provider
    pub fn new_remote(
        pool_name: String,
        ssh_manager: Arc<crate::core::SshManager>,
        host: String,
        port: u16,
        user: String,
        credential: SshCredential,
    ) -> Self {
        Self {
            pool_name,
            ssh_manager: Some(ssh_manager),
            ssh_target: Some((host, port, user, credential)),
        }
    }

    // Conditional compilation for execute_command
    #[cfg(not(test))]
    async fn execute_command(&self, command: &str, description: &str) -> Result<CommandOutput> {
        if let (Some(manager), Some((host, port, user, credential))) =
            (&self.ssh_manager, &self.ssh_target)
        {
            manager
                .execute(host, *port, user, credential, command)
                .await
                .map_err(|e| anyhow::anyhow!("Remote ZFS command failed on {}: {}", host, e))
        } else {
            run_shell_command(command, description)
                .await
                .map_err(|e| anyhow::anyhow!("Local ZFS command failed: {}", e))
        }
    }

    #[cfg(test)]
    async fn execute_command(&self, command: &str, description: &str) -> Result<CommandOutput> {
        // Mock implementation for testing
        Ok(CommandOutput {
            stdout: format!("Mock output for: {}", command),
            stderr: String::new(),
            exit_code: 0,
        })
    }

    /// Check if ZFS pool exists
    async fn pool_exists(&self) -> Result<bool> {
        let command = format!("zpool list -H -o name {}", self.pool_name);
        match self.execute_command(&command, "Check if ZFS pool exists").await {
            Ok(output) => Ok(output.stdout.trim() == self.pool_name),
            Err(_) => Ok(false),
        }
    }

    /// Check if ZFS dataset/volume exists
    async fn dataset_exists(&self, dataset_name: &str) -> Result<bool> {
        let command = format!("zfs list -H -o name {}", dataset_name);
        match self.execute_command(&command, "Check if ZFS dataset exists").await {
            Ok(output) => Ok(output.stdout.trim() == dataset_name),
            Err(_) => Ok(false),
        }
    }

    /// Get the device path for a ZFS volume
    async fn get_volume_device_path(&self, volume_name: &str) -> Result<String> {
        let dataset_name = format!("{}/{}", self.pool_name, volume_name);
        let command = format!("zfs get -H -o value volmode {}", dataset_name);

        let volmode = self.execute_command(&command, "Get ZFS volume mode").await?;
        if volmode.stdout.trim() != "full" {
            // Set volmode to full if not already set
            let set_command = format!("zfs set volmode=full {}", dataset_name);
            self.execute_command(&set_command, "Set ZFS volume mode to full").await?;
        }

        // ZFS volume device path is typically /dev/zvol/<pool>/<volume>
        Ok(format!("/dev/zvol/{}/{}", self.pool_name, volume_name))
    }
}

#[async_trait]
impl StorageProvider for ZfsProvider {
    async fn init_pool(&self, _disk: &str) -> Result<()> {
        // ZFS pool initialization is typically done outside of this provider
        // as it requires entire disks and is a destructive operation
        warn!("ZFS pool initialization should be done manually. Pool '{}' should already exist.", self.pool_name);
        Ok(())
    }

    async fn create_volume(&self, vol_name: &str, size_gb: u64) -> Result<String> {
        info!(
            "Creating ZFS volume '{}' of size {}GB in pool '{}'",
            vol_name, size_gb, self.pool_name
        );

        // Check if pool exists
        if !self.pool_exists().await? {
            return Err(anyhow::anyhow!("ZFS pool '{}' does not exist", self.pool_name));
        }

        let dataset_name = format!("{}/{}", self.pool_name, vol_name);

        // Check if volume already exists
        if self.dataset_exists(&dataset_name).await? {
            warn!(
                "ZFS volume '{}' already exists in pool '{}', reusing it",
                vol_name, self.pool_name
            );
            return self.get_volume_device_path(vol_name).await;
        }

        // Create ZFS volume
        // -V specifies volume size, -b specifies block size (default 128K is good for most workloads)
        let command = format!(
            "zfs create -V {}G -b 128K {}",
            size_gb, dataset_name
        );

        self.execute_command(&command, "Create ZFS volume").await?;

        // Get the device path
        let device_path = self.get_volume_device_path(vol_name).await?;

        info!(
            "Successfully created ZFS volume '{}' at '{}'",
            vol_name, device_path
        );

        Ok(device_path)
    }

    async fn delete_volume(&self, vol_name: &str) -> Result<()> {
        info!(
            "Deleting ZFS volume '{}' from pool '{}'",
            vol_name, self.pool_name
        );

        let dataset_name = format!("{}/{}", self.pool_name, vol_name);

        // Check if volume exists
        if !self.dataset_exists(&dataset_name).await? {
            warn!(
                "ZFS volume '{}' does not exist in pool '{}'",
                vol_name, self.pool_name
            );
            return Ok(());
        }

        // Force destroy the volume and any snapshots
        let command = format!("zfs destroy -Rf {}", dataset_name);
        self.execute_command(&command, "Delete ZFS volume").await?;

        info!("Successfully deleted ZFS volume '{}'", vol_name);
        Ok(())
    }

    async fn resize_volume(&self, vol_name: &str, new_size_gb: u64) -> Result<()> {
        info!(
            "Resizing ZFS volume '{}' to {}GB in pool '{}'",
            vol_name, new_size_gb, self.pool_name
        );

        let dataset_name = format!("{}/{}", self.pool_name, vol_name);

        // Check if volume exists
        if !self.dataset_exists(&dataset_name).await? {
            return Err(anyhow::anyhow!(
                "ZFS volume '{}' does not exist in pool '{}'",
                vol_name, self.pool_name
            ));
        }

        // Resize ZFS volume
        let command = format!("zfs set volsize={}G {}", new_size_gb, dataset_name);
        self.execute_command(&command, "Resize ZFS volume").await?;

        info!("Successfully resized ZFS volume '{}' to {}GB", vol_name, new_size_gb);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require ZFS to be installed and a test pool to be available
    // They should be run in a test environment with proper ZFS setup

    #[tokio::test]
    #[ignore] // Ignore by default as it requires ZFS setup
    async fn test_zfs_provider_local() {
        let provider = ZfsProvider::new_local("testpool".to_string());

        // Test requires a test pool to exist
        // This test should be adapted to your test environment
        assert!(provider.pool_exists().await.unwrap_or(false));
    }

    #[tokio::test]
    #[ignore] // Ignore by default as it requires ZFS setup
    async fn test_zfs_volume_operations() {
        let provider = ZfsProvider::new_local("testpool".to_string());
        let vol_name = "test_volume";

        // This test should be adapted to your test environment
        // and proper cleanup should be ensured
    }
}
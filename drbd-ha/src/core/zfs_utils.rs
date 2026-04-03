//! ZFS utility wrapper
//!
//! Wraps zfs-utils crate.

use crate::core::SshManager;
use ssh_cmd::SshCredential;
use std::sync::Arc;

// Re-export Zpool types
pub use zfs_utils::{ZfsDatasetInfo, ZfsPoolInfo, ZpoolCheckResult, ZpoolStatus};

/// Client for querying ZFS information (local or remote)
pub struct ZfsClient {
    inner: zfs_utils::ZfsClient,
}

impl ZfsClient {
    /// Create a new local ZFS client
    pub fn new_local() -> Self {
        Self {
            inner: zfs_utils::ZfsClient::new_local(),
        }
    }

    /// Create a new remote ZFS client
    pub fn new_remote(
        ssh_manager: Arc<SshManager>,
        host: String,
        port: u16,
        user: String,
        credential: SshCredential,
    ) -> Self {
        // Convert SshManager wrapper to inner ssh_cmd::SshManager
        let inner_manager = ssh_manager.to_inner();

        Self {
            inner: zfs_utils::ZfsClient::new_remote(
                Arc::new(inner_manager),
                host,
                port,
                user,
                credential,
            ),
        }
    }

    /// Check if zpool is installed and available
    pub async fn check_zpool(&self) -> anyhow::Result<ZpoolCheckResult> {
        self.inner
            .check_zpool()
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Get info about a specific zpool
    pub async fn get_pool_info(&self, pool_name: &str) -> anyhow::Result<Option<ZfsPoolInfo>> {
        self.inner
            .get_pool_info(pool_name)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// List all zpools
    pub async fn list_pool_info(&self) -> anyhow::Result<Vec<ZfsPoolInfo>> {
        self.inner
            .list_pool_info()
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Get backing devices for a zpool
    pub async fn get_pool_devices(&self, pool_name: &str) -> anyhow::Result<Vec<String>> {
        self.inner
            .get_pool_devices(pool_name)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// List datasets, optionally scoped to a specific pool
    pub async fn list_datasets(
        &self,
        pool_name: Option<&str>,
    ) -> anyhow::Result<Vec<ZfsDatasetInfo>> {
        self.inner
            .list_datasets(pool_name)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Create a zpool from the provided devices
    pub async fn create_pool(&self, pool_name: &str, devices: &[String]) -> anyhow::Result<()> {
        self.inner
            .create_pool(pool_name, devices)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }
}

/// Check if zpool is installed and available (Local).
pub async fn check_zpool() -> anyhow::Result<ZpoolCheckResult> {
    zfs_utils::check_zpool()
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

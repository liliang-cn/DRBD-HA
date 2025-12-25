//! ZFS utility wrapper
//!
//! Wraps zfs-utils crate.

use crate::core::SshManager;
use ssh_cmd::SshCredential;
use std::sync::Arc;

// Re-export Zpool types
pub use zfs_utils::{ZpoolCheckResult, ZpoolStatus};

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
}

/// Check if zpool is installed and available (Local).
pub async fn check_zpool() -> anyhow::Result<ZpoolCheckResult> {
    zfs_utils::check_zpool()
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

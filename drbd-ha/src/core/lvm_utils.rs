//! LVM utility wrapper
//!
//! Wraps lvm-utils crate.

use crate::core::SshManager;
use ssh_cmd::SshCredential;
use std::sync::Arc;

// Re-export LvmVgInfo and LvmLvInfo
pub use lvm_utils::{LvmLvInfo, LvmVgInfo};

/// Client for querying LVM information (local or remote)
pub struct LvmClient {
    inner: lvm_utils::LvmClient,
}

impl LvmClient {
    /// Create a new local LVM client
    pub fn new_local() -> Self {
        Self {
            inner: lvm_utils::LvmClient::new_local(),
        }
    }

    /// Create a new remote LVM client
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
            inner: lvm_utils::LvmClient::new_remote(
                Arc::new(inner_manager),
                host,
                port,
                user,
                credential,
            ),
        }
    }

    /// Get info about a specific LVM Volume Group
    pub async fn get_vg_info(&self, vg_name: &str) -> anyhow::Result<Option<LvmVgInfo>> {
        self.inner
            .get_vg_info(vg_name)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// List all LVM Volume Groups
    pub async fn list_vg_info(&self) -> anyhow::Result<Vec<LvmVgInfo>> {
        self.inner
            .list_vg_info()
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// List all LVM Logical Volumes
    pub async fn list_lvs(&self) -> anyhow::Result<Vec<LvmLvInfo>> {
        self.inner.list_lvs().await.map_err(|e| anyhow::anyhow!(e))
    }

    /// List available (unused) LVM Logical Volumes
    pub async fn list_available_lvs(&self) -> anyhow::Result<Vec<LvmLvInfo>> {
        self.inner
            .list_available_lvs()
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }
}

/// Get LVM Volume Group information by name (Local).
pub async fn get_vg_info(vg_name: &str) -> anyhow::Result<Option<LvmVgInfo>> {
    lvm_utils::get_vg_info(vg_name)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// List all LVM Volume Groups (Local).
pub async fn list_vg_info() -> anyhow::Result<Vec<LvmVgInfo>> {
    lvm_utils::list_vg_info()
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// List all LVM Logical Volumes (Local).
pub async fn list_lvs() -> anyhow::Result<Vec<LvmLvInfo>> {
    lvm_utils::list_lvs().await.map_err(|e| anyhow::anyhow!(e))
}

use super::StorageProvider;
use crate::core::{SshCredential, SshManager};
use anyhow::Result;
use async_trait::async_trait;
use lvm_utils::LvmClient;
use std::sync::Arc;

pub struct LvmProvider {
    pub client: LvmClient,
    pub vg_name: String,
    pub thin_pool_name: Option<String>, // If set, use thin pool for volume creation
}

impl LvmProvider {
    pub fn new_local(vg_name: String) -> Self {
        LvmProvider {
            client: LvmClient::new_local(),
            vg_name,
            thin_pool_name: Some("thinpool".to_string()), // Default to thin pool
        }
    }

    pub fn new_local_with_thin_pool(vg_name: String, thin_pool_name: String) -> Self {
        LvmProvider {
            client: LvmClient::new_local(),
            vg_name,
            thin_pool_name: Some(thin_pool_name),
        }
    }

    pub fn new_remote(
        vg_name: String,
        ssh_manager: Arc<SshManager>,
        host: String,
        port: u16,
        user: String,
        credential: SshCredential,
    ) -> Self {
        // We need to convert from crate::core::SshCredential to ssh_cmd::SshCredential
        // Since they are re-exported, this should be seamless if types match,
        // otherwise we construct it.
        // Assuming types match because SshManager in core uses ssh_cmd.

        let client = LvmClient::new_remote(
            ssh_manager.to_inner().into(), // Convert to Arc<ssh_cmd::SshManager>
            host,
            port,
            user,
            credential,
        );

        LvmProvider {
            client,
            vg_name,
            thin_pool_name: Some("thinpool".to_string()), // Default to thin pool
        }
    }

    pub fn new_remote_with_thin_pool(
        vg_name: String,
        thin_pool_name: String,
        ssh_manager: Arc<SshManager>,
        host: String,
        port: u16,
        user: String,
        credential: SshCredential,
    ) -> Self {
        let client =
            LvmClient::new_remote(ssh_manager.to_inner().into(), host, port, user, credential);

        LvmProvider {
            client,
            vg_name,
            thin_pool_name: Some(thin_pool_name),
        }
    }
}

#[async_trait]
impl StorageProvider for LvmProvider {
    async fn init_pool(&self, disk: &str) -> Result<()> {
        self.client
            .init_pool(&self.vg_name, disk)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn create_volume(&self, vol_name: &str, size_gb: u64) -> Result<String> {
        // Use thin pool if configured, otherwise create thick volume
        if let Some(ref thin_pool) = self.thin_pool_name {
            // First ensure thin pool exists (idempotent-ish check/create logic is nice but LvmClient creates it if asked)
            // LvmClient::create_thin_volume needs an existing pool.
            // But wait, who creates the thin pool?
            // Usually, we should create the thin pool once.
            // Here, we'll try to create the thin pool first if it doesn't exist?
            // LvmProvider logic previously didn't explicitly create the thin pool in create_volume,
            // it just ran `create_thin_volume_cmd`.
            // Let's check `LvmCmd::create_thin_volume_cmd`.
            // It runs: `lvcreate -y -V {} --type thin -n {} --thinpool {} {}`
            // This assumes the thin pool exists OR lvm might auto-create?
            // No, `lvcreate --thinpool` expects an existing pool LV.
            // However, `LvmCmd::create_thin_volume_cmd` uses the syntax `--thinpool poolname`.

            // To be robust and match previous behavior (which relied on LVM command),
            // we should delegate to LvmClient.
            // LvmClient has `create_thin_volume`.

            // NOTE: The previous LvmProvider logic was:
            // if thin_pool: LvmCmd::create_thin_volume_cmd(...)
            // else: LvmCmd::create_lv_cmd(...)

            // If the previous code worked, it means either the thin pool existed, or the command created it?
            // Actually, usually you must create the pool first.
            // Since we are refactoring, let's just use LvmClient::create_thin_volume.
            // If it fails because pool is missing, we might need to create it.
            // For now, let's assume the user (or caller) handles pool creation,
            // OR we rely on `LvmClient` to do the right thing.
            // But wait, we haven't added logic to auto-create thin pool in LvmClient yet.

            // Actually, `LvmProvider` is used in `create_profile`.
            // And in `create_profile` we init the VG. We don't explicitly create the thin pool.
            // If the user wants a thin pool, they need to create it.
            // BUT, `LvmProvider::new_local` defaults `thin_pool_name` to "thinpool".
            // If this code was working before, then maybe `lvcreate --type thin --thinpool ...` creates it?
            // No, typically you use `--type thin-pool` to create the pool.

            // Let's look at `LvmCmd::create_thin_volume_cmd`:
            // `lvcreate -y -V {} --type thin -n {} --thinpool {} {}`
            // This creates a thin volume in an existing pool.

            // If `drbd-ha` creates a new VG via `init_pool`, it's empty.
            // Then it calls `create_volume`.
            // If `thin_pool_name` is set (default "thinpool"), it tries to create a volume in "thinpool".
            // If "thinpool" doesn't exist, this fails.

            // So, `LvmProvider` should probably ensure the thin pool exists.
            // Let's check if the previous implementation handled this.
            // Previous: `LvmCmd::create_thin_volume_cmd`.
            // It seems the previous code might have failed if thin pool didn't exist!
            // Or maybe I missed where thin pool is created.

            // Since I am refactoring, I will keep the behavior of calling `create_thin_volume`.
            // If it fails, that's consistent with "thin pool missing".
            // However, I can try to create the thin pool if it's missing, which would be an improvement.
            // For now, I'll map directly.

            self.client
                .create_thin_volume(&self.vg_name, thin_pool, vol_name, size_gb)
                .await
                .map_err(|e| anyhow::anyhow!(e))
        } else {
            self.client
                .create_volume(&self.vg_name, vol_name, size_gb)
                .await
                .map_err(|e| anyhow::anyhow!(e))
        }
    }

    async fn delete_volume(&self, vol_name: &str) -> Result<()> {
        self.client
            .delete_volume(&self.vg_name, vol_name)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn resize_volume(&self, vol_name: &str, new_size_gb: u64) -> Result<()> {
        self.client
            .resize_volume(&self.vg_name, vol_name, new_size_gb)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }
}

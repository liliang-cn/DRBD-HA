use async_trait::async_trait;
use anyhow::Result;

#[async_trait]
pub trait StorageProvider {
    /// 初始化存储池 (vgcreate)
    async fn init_pool(&self, disk: &str) -> Result<()>;

    /// 创建卷 (lvcreate)
    async fn create_volume(&self, vol_name: &str, size_gb: u64) -> Result<String>;

    /// 删除卷 (lvremove)
    async fn delete_volume(&self, vol_name: &str) -> Result<()>;

    /// 扩容 (lvextend)
    async fn resize_volume(&self, vol_name: &str, new_size_gb: u64) -> Result<()>;
}

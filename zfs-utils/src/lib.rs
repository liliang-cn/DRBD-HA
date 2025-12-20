pub mod client;
mod cmd;
pub mod config;
pub mod error;
#[cfg(test)]
pub mod mock;

pub use client::{
    get_pool_info, get_thin_volume_info, list_datasets, list_pool_info, list_thin_volumes,
    ZfsClient, ZfsDatasetInfo, ZfsPoolInfo, ZfsThinVolumeInfo,
};
pub use config::configure_zfs_cache;
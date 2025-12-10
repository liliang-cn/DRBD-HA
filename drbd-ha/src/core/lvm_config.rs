//! LVM configuration wrapper
//!
//! Wraps lvm-utils crate.

/// Configures the LVM filter in /etc/lvm/lvm.conf to prevent scanning DRBD devices.
pub async fn configure_lvm_filter() -> anyhow::Result<()> {
    lvm_utils::configure_lvm_filter()
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

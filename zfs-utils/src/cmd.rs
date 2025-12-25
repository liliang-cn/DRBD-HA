use crate::error::{ZfsError, ZfsResult};
use ssh_cmd::CommandOutput;
use std::process::Stdio;
use tokio::process::Command;

/// ZFS command builder for generating ZFS command strings
/// This is useful when you need to execute commands via SSH or build custom command chains
pub struct ZfsCmd;

impl ZfsCmd {
    /// List zpool (check if exists)
    pub fn zpool_list_cmd(pool_name: &str) -> String {
        format!("zpool list {}", pool_name)
    }

    /// Create zpool
    pub fn zpool_create_cmd(pool_name: &str, disk: &str) -> String {
        format!("zpool create -f {} {}", pool_name, disk)
    }

    /// Create ZFS volume (sparse/thin)
    pub fn zfs_create_sparse_volume_cmd(pool_name: &str, volume_name: &str, size_gb: &str) -> String {
        format!("zfs create -s -V {}G -b 128K {}/{}", size_gb, pool_name, volume_name)
    }

    /// Create ZFS volume (regular/thick)
    pub fn zfs_create_volume_cmd(pool_name: &str, volume_name: &str, size_gb: &str) -> String {
        format!("zfs create -V {}G -b 128K {}/{}", size_gb, pool_name, volume_name)
    }

    /// Destroy ZFS dataset recursively
    pub fn zfs_destroy_cmd(dataset_name: &str) -> String {
        format!("zfs destroy -Rf {}", dataset_name)
    }

    /// List ZFS dataset (check if exists)
    pub fn zfs_list_cmd(dataset_name: &str) -> String {
        format!("zfs list -H -o name {}", dataset_name)
    }

    /// Get zpool status
    pub fn zpool_status_cmd() -> String {
        "zpool status".to_string()
    }

    /// List all zpools
    pub fn zpool_list_all_cmd() -> String {
        "zpool list -H -o name,size,free,health".to_string()
    }
}

#[allow(dead_code)]
pub async fn run_local_command(command: &str, description: &str) -> ZfsResult<CommandOutput> {
    tracing::info!("Executing: {} ({})", command, description);

    let parts = shlex::split(command).ok_or_else(|| {
        ZfsError::Execution(format!("Failed to parse command: {}", command))
    })?;

    let mut cmd = Command::new(&parts[0]);
    if parts.len() > 1 {
        cmd.args(&parts[1..]);
    }

    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| ZfsError::Execution(format!("Failed to execute command: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(CommandOutput {
        stdout,
        stderr,
        exit_code: output.status.code().unwrap_or(-1),
    })
}
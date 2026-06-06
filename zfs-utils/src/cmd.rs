use crate::error::{ZfsError, ZfsResult};
use ssh_cmd::CommandOutput;
use std::process::Stdio;
use tokio::process::Command;

const REMOTE_EXEC_HOST_ENV: &str = "DRBD_HA_REMOTE_EXEC_HOST";
const REMOTE_EXEC_PORT_ENV: &str = "DRBD_HA_REMOTE_EXEC_PORT";
const REMOTE_EXEC_USER_ENV: &str = "DRBD_HA_REMOTE_EXEC_USER";

#[derive(Debug, Clone)]
struct ProxyTarget {
    host: String,
    port: u16,
    user: String,
}

fn proxy_target_from_env() -> Option<ProxyTarget> {
    let host = std::env::var(REMOTE_EXEC_HOST_ENV).ok()?;
    let port = std::env::var(REMOTE_EXEC_PORT_ENV)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(22);
    let user = std::env::var(REMOTE_EXEC_USER_ENV).unwrap_or_else(|_| "root".to_string());

    Some(ProxyTarget { host, port, user })
}

fn shell_escape_single_quotes(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

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
    pub fn zfs_create_sparse_volume_cmd(
        pool_name: &str,
        volume_name: &str,
        size_gb: &str,
    ) -> String {
        format!(
            "zfs create -s -V {}G -b 128K {}/{}",
            size_gb, pool_name, volume_name
        )
    }

    /// Create ZFS volume (regular/thick)
    pub fn zfs_create_volume_cmd(pool_name: &str, volume_name: &str, size_gb: &str) -> String {
        format!(
            "zfs create -V {}G -b 128K {}/{}",
            size_gb, pool_name, volume_name
        )
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

    if let Some(proxy) = proxy_target_from_env() {
        // Remote: keep login shell + sudo, run via dispatch.
        let escaped_cmd = shell_escape_single_quotes(command);
        let remote_cmd = if proxy.user == "root" {
            format!("sh -lc '{}'", escaped_cmd)
        } else {
            format!("sudo -n sh -lc '{}'", escaped_cmd)
        };
        let cfg = dispatch::Config {
            ssh_config_path: Some(std::path::PathBuf::from("/dev/null")),
            config_path: Some(std::path::PathBuf::from("/dev/null")),
            host_key_checking: dispatch::HostKeyChecking::AcceptAny,
            known_hosts_file: Some(std::path::PathBuf::from("/dev/null")),
            connect_timeout: Some(std::time::Duration::from_secs(5)),
            ..Default::default()
        };
        let client = dispatch::Dispatch::new(cfg)
            .map_err(|e| ZfsError::Execution(format!("dispatch init failed: {}", e)))?;
        let dest = format!("ssh://{}@{}:{}", proxy.user, proxy.host, proxy.port);
        let result = client
            .exec([dest], remote_cmd)
            .run()
            .await
            .map_err(|e| ZfsError::Execution(format!("dispatch exec failed: {}", e)))?;
        let hr = result
            .hosts
            .into_values()
            .next()
            .ok_or_else(|| ZfsError::Execution("dispatch returned no host result".to_string()))?;
        Ok(CommandOutput {
            stdout: hr.stdout,
            stderr: hr.stderr,
            exit_code: hr.exit_code,
        })
    } else {
        let parts = shlex::split(command)
            .ok_or_else(|| ZfsError::Execution(format!("Failed to parse command: {}", command)))?;

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
        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

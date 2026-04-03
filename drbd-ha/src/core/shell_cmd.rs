use crate::error::{AppError, AppResult};
pub use shell_cmd::CommandOutput;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct CommandProxyConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
}

const REMOTE_EXEC_HOST_ENV: &str = "DRBD_HA_REMOTE_EXEC_HOST";
const REMOTE_EXEC_PORT_ENV: &str = "DRBD_HA_REMOTE_EXEC_PORT";
const REMOTE_EXEC_USER_ENV: &str = "DRBD_HA_REMOTE_EXEC_USER";

pub fn configure_command_proxy(config: Option<CommandProxyConfig>) {
    if let Some(proxy) = config {
        std::env::set_var(REMOTE_EXEC_HOST_ENV, proxy.host);
        std::env::set_var(REMOTE_EXEC_PORT_ENV, proxy.port.to_string());
        std::env::set_var(REMOTE_EXEC_USER_ENV, proxy.user);
    } else {
        std::env::remove_var(REMOTE_EXEC_HOST_ENV);
        std::env::remove_var(REMOTE_EXEC_PORT_ENV);
        std::env::remove_var(REMOTE_EXEC_USER_ENV);
    }
}

pub fn current_command_proxy() -> Option<CommandProxyConfig> {
    let host = std::env::var(REMOTE_EXEC_HOST_ENV).ok()?;
    let port = std::env::var(REMOTE_EXEC_PORT_ENV)
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(22);
    let user = std::env::var(REMOTE_EXEC_USER_ENV).unwrap_or_else(|_| "root".to_string());

    Some(CommandProxyConfig { host, port, user })
}

fn shell_escape_single_quotes(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

async fn run_shell_command_via_proxy(
    proxy: &CommandProxyConfig,
    cmd_str: &str,
) -> AppResult<CommandOutput> {
    let target = format!("{}@{}", proxy.user, proxy.host);
    let escaped_cmd = shell_escape_single_quotes(cmd_str);
    let remote_cmd = if proxy.user == "root" {
        format!("sh -lc '{}'", escaped_cmd)
    } else {
        format!("sudo -n sh -lc '{}'", escaped_cmd)
    };

    let output = Command::new("ssh")
        .arg("-p")
        .arg(proxy.port.to_string())
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("UserKnownHostsFile=/dev/null")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=5")
        .arg(target)
        .arg(remote_cmd)
        .output()
        .await
        .map_err(|e| {
            AppError::Internal(format!("Failed to execute proxied shell command: {}", e))
        })?;

    Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Executes a shell command and returns its output.
///
/// # Arguments
/// * `cmd_str` - The command string to execute.
/// * `description` - A brief description of the command's purpose for logging.
pub async fn run_shell_command(cmd_str: &str, description: &str) -> AppResult<CommandOutput> {
    if let Some(proxy) = current_command_proxy() {
        if !description.is_empty() {
            tracing::debug!(
                "Executing proxied command via {}@{}:{}: '{}' ({})",
                proxy.user,
                proxy.host,
                proxy.port,
                cmd_str,
                description
            );
        }
        run_shell_command_via_proxy(&proxy, cmd_str).await
    } else {
        shell_cmd::run_shell_command(cmd_str, description)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))
    }
}

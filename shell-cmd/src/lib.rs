pub mod error;

use crate::error::{ShellError, ShellResult};
use tokio::process::Command;
use tracing::{debug, error};

const REMOTE_EXEC_HOST_ENV: &str = "DRBD_HA_REMOTE_EXEC_HOST";
const REMOTE_EXEC_PORT_ENV: &str = "DRBD_HA_REMOTE_EXEC_PORT";
const REMOTE_EXEC_USER_ENV: &str = "DRBD_HA_REMOTE_EXEC_USER";

/// Command execution output
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

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

impl CommandOutput {
    /// Check if command succeeded (exit code 0)
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Executes a shell command and returns its output.
///
/// # Arguments
/// * `cmd_str` - The command string to execute.
/// * `description` - A brief description of the command's purpose for logging.
pub async fn run_shell_command(cmd_str: &str, description: &str) -> ShellResult<CommandOutput> {
    // Only log if there's a description (to avoid SSE polling noise)
    if !description.is_empty() {
        debug!("Executing command: '{}' ({})", cmd_str, description);
    }

    let output = if let Some(proxy) = proxy_target_from_env() {
        let target = format!("{}@{}", proxy.user, proxy.host);
        let escaped_cmd = shell_escape_single_quotes(cmd_str);
        let remote_cmd = if proxy.user == "root" {
            format!("sh -lc '{}'", escaped_cmd)
        } else {
            format!("sudo -n sh -lc '{}'", escaped_cmd)
        };

        Command::new("ssh")
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
            .map_err(ShellError::Io)?
    } else {
        Command::new("sh")
            .arg("-c")
            .arg(cmd_str)
            .output()
            .await
            .map_err(ShellError::Io)?
    };

    let cmd_output = CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    };

    if !cmd_output.success() {
        // Only log errors, not successful status checks
        error!(
            "Command '{}' failed with exit code {}. Stderr: '{}'",
            cmd_str, cmd_output.exit_code, cmd_output.stderr
        );
    }

    Ok(cmd_output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_run_shell_command_success() {
        let result = run_shell_command("echo hello", "test echo").await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success());
        assert_eq!(output.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_run_shell_command_failure() {
        let result = run_shell_command("ls /nonexistent/path", "test failure").await;
        assert!(result.is_ok()); // Command runs, but returns failure exit code
        let output = result.unwrap();
        assert!(!output.success());
        assert!(output.exit_code != 0);
    }
}

pub mod error;

use crate::error::{ShellError, ShellResult};
use tokio::process::Command;
use tracing::{debug, error};

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
    dispatch_config::command_proxy().map(|p| ProxyTarget {
        host: p.host,
        port: p.port,
        user: p.user,
    })
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

    let cmd_output = if let Some(proxy) = proxy_target_from_env() {
        // Remote: keep the login shell + sudo exactly as before, run via dispatch.
        let escaped_cmd = shell_escape_single_quotes(cmd_str);
        let remote_cmd = if proxy.user == "root" {
            format!("sh -lc '{}'", escaped_cmd)
        } else {
            format!("sudo -n sh -lc '{}'", escaped_cmd)
        };
        let cfg = dispatch_config::default_dispatch_config();
        let client = dispatch::Dispatch::new(cfg)
            .map_err(|e| ShellError::Execution(format!("dispatch init failed: {}", e)))?;
        let dest = format!("ssh://{}@{}:{}", proxy.user, proxy.host, proxy.port);
        let result = client
            .exec([dest], remote_cmd)
            .run()
            .await
            .map_err(|e| ShellError::Execution(format!("dispatch exec failed: {}", e)))?;
        let hr = result
            .hosts
            .into_values()
            .next()
            .ok_or_else(|| ShellError::Execution("dispatch returned no host result".into()))?;
        CommandOutput {
            stdout: hr.stdout,
            stderr: hr.stderr,
            exit_code: hr.exit_code,
        }
    } else {
        // Local execution stays a local process.
        let output = Command::new("sh")
            .arg("-c")
            .arg(cmd_str)
            .output()
            .await
            .map_err(ShellError::Io)?;
        CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        }
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

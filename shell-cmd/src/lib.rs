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

    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd_str)
        .output()
        .await
        .map_err(ShellError::Io)?;

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

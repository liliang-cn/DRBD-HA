use anyhow::Context;
use tokio::process::Command;
use tracing::{info, debug, error};
use crate::error::{AppError, AppResult};

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
pub async fn run_shell_command(cmd_str: &str, description: &str) -> AppResult<CommandOutput> {
    info!("Executing command: '{}' ({})", cmd_str, description);

    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd_str)
        .output()
        .await
        .context(format!("Failed to execute command: '{}'", cmd_str))
        .map_err(|e| AppError::Internal(format!("{}", e)))?;

    let cmd_output = CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    };

    if cmd_output.success() {
        debug!(
            "Command '{}' succeeded. Stdout: '{}', Stderr: '{}'",
            cmd_str,
            cmd_output.stdout,
            cmd_output.stderr
        );
    } else {
        error!(
            "Command '{}' failed with exit code {}. Stdout: '{}', Stderr: '{}'",
            cmd_str,
            cmd_output.exit_code,
            cmd_output.stdout,
            cmd_output.stderr
        );
    }

    Ok(cmd_output)
}
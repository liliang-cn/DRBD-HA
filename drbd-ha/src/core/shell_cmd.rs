use crate::error::{AppError, AppResult};
pub use shell_cmd::CommandOutput;

/// Executes a shell command and returns its output.
///
/// # Arguments
/// * `cmd_str` - The command string to execute.
/// * `description` - A brief description of the command's purpose for logging.
pub async fn run_shell_command(cmd_str: &str, description: &str) -> AppResult<CommandOutput> {
    shell_cmd::run_shell_command(cmd_str, description)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))
}

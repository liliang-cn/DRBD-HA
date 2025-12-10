use crate::error::{LvmError, LvmResult};
use ssh_cmd::CommandOutput;
use tokio::process::Command;
use tracing::{debug, error, info};

#[allow(dead_code)]
pub async fn run_local_command(cmd_str: &str, description: &str) -> LvmResult<CommandOutput> {
    info!("Executing local command: '{}' ({})", cmd_str, description);

    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd_str)
        .output()
        .await
        .map_err(LvmError::Io)?;

    let cmd_output = CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    };

    if cmd_output.success() {
        debug!(
            "Command '{}' succeeded. Stdout: '{}', Stderr: '{}'",
            cmd_str, cmd_output.stdout, cmd_output.stderr
        );
    } else {
        error!(
            "Command '{}' failed with exit code {}. Stdout: '{}', Stderr: '{}'",
            cmd_str, cmd_output.exit_code, cmd_output.stdout, cmd_output.stderr
        );
    }

    Ok(cmd_output)
}

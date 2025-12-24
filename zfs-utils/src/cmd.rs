use crate::error::{ZfsError, ZfsResult};
use ssh_cmd::CommandOutput;
use std::process::Stdio;
use tokio::process::Command;

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
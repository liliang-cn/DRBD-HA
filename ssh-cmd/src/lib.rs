pub mod config;
pub mod error;
#[cfg(test)]
pub mod mock;

use crate::config::SshConfig;
use crate::error::{SshError, SshResult};
#[cfg(not(test))]
use std::process::Stdio;
#[cfg(not(test))]
use tokio::process::Command;

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

/// SSH connection credentials
/// (Kept for API compatibility, but ignored in the current implementation which uses system ssh)
#[derive(Debug, Clone)]
pub enum SshCredential {
    /// SSH private key (PEM format)
    PrivateKey(String),
    /// SSH password
    Password(String),
}

/// SSH connection manager
pub struct SshManager {
    config: SshConfig,
}

impl SshManager {
    /// Create a new SSH manager with the given configuration
    pub fn new(config: SshConfig) -> Self {
        Self { config }
    }

    /// Get the configuration
    pub fn config(&self) -> &SshConfig {
        &self.config
    }

    /// Get the session key for a host (reserved for connection pooling)
    pub fn session_key(host: &str, port: u16, user: &str) -> String {
        format!("{}@{}:{}", user, host, port)
    }

    /// Execute a command on a remote host

    pub async fn execute(
        &self,

        host: &str,

        port: u16,

        user: &str,

        _credential: &SshCredential,

        command: &str,
    ) -> SshResult<CommandOutput> {
        #[cfg(test)]
        {
            // Avoid unused variable warnings

            let _ = host;

            let _ = port;

            let _ = user;

            crate::mock::tests::record_command(command.to_string());

            if let Some(output) = crate::mock::tests::get_next_mock_output() {
                return Ok(output);
            }

            panic!(
                "Test tried to execute command without mock output: {}",
                command
            );
        }

        #[cfg(not(test))]
        {
            // We use the system ssh command.

            // Assumes keys are set up in the environment (e.g. ssh-agent or default keys).

            // Ignores credential.

            let target = format!("{}@{}", user, host);

            // For non-root users, wrap privileged commands with sudo

            let final_command = if user != "root" && Self::needs_sudo(command) {
                format!("sudo -n {}", command)
            } else {
                command.to_string()
            };

            tracing::info!(
                "SSH execute: host={}, port={}, user={}, command='{}'",
                host,
                port,
                user,
                final_command
            );

            // Build the command

            // ssh -p <port> -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o BatchMode=yes <target> <command>

            let mut cmd = Command::new("ssh");

            cmd.arg("-p")
                .arg(port.to_string())
                .arg("-o")
                .arg("StrictHostKeyChecking=no")
                .arg("-o")
                .arg("UserKnownHostsFile=/dev/null")
                .arg("-o")
                .arg("BatchMode=yes")
                .arg("-o")
                .arg("ConnectTimeout=5")
                .arg(&target)
                .arg(&final_command);

            // Set timeout from config if needed, but here we use a flag or tokio timeout

            // Using tokio timeout for the whole operation

            let child = cmd
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| SshError::Execution(format!("Failed to spawn ssh command: {}", e)))?;

            let output =
                tokio::time::timeout(self.config.connection_timeout(), child.wait_with_output())
                    .await
                    .map_err(|_| SshError::Timeout(format!("Connection timeout to {}", host)))?
                    .map_err(|e| {
                        SshError::Execution(format!("Failed to execute ssh command: {}", e))
                    })?;

            tracing::debug!(
                "SSH result: host={}, exit_code={}, stdout_len={}, stderr_len={}",
                host,
                output.status.code().unwrap_or(-1),
                output.stdout.len(),
                output.stderr.len()
            );

            Ok(CommandOutput {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),

                stderr: String::from_utf8_lossy(&output.stderr).to_string(),

                exit_code: output.status.code().unwrap_or(-1),
            })
        }
    }

    /// Check if a command needs sudo privileges
    #[allow(dead_code)]
    fn needs_sudo(command: &str) -> bool {
        // Commands that typically need root/sudo privileges
        const PRIVILEGED_COMMANDS: &[&str] = &[
            "lsblk",
            "pvs",
            "vgs",
            "lvs",
            "pvcreate",
            "vgcreate",
            "lvcreate",
            "pvremove",
            "vgremove",
            "lvremove",
            "lvextend",
            "lvreduce",
            "drbdadm",
            "drbdsetup",
            "drbdmeta",
            "systemctl",
            "journalctl",
            "tee",
            "dd",
            "mkfs",
            "mount",
            "umount",
            "ip",
            "iptables",
            "ufw",
            "targetcli",
            "nvme",
            "mv",    // Moving files in /etc requires sudo
            "cp",    // Copying files in /etc requires sudo
            "rm",    // Removing files in /etc requires sudo
            "chown", // Changing ownership requires sudo
            "chmod", // Changing permissions requires sudo
        ];

        // Check if command starts with any privileged command
        let cmd_lower = command.trim().to_lowercase();
        PRIVILEGED_COMMANDS.iter().any(|&priv_cmd| {
            cmd_lower.starts_with(priv_cmd) || cmd_lower.contains(&format!(" {}", priv_cmd))
        })
    }

    /// Execute a command and parse JSON output
    pub async fn execute_json<T: serde::de::DeserializeOwned>(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        command: &str,
    ) -> SshResult<T> {
        let output = self.execute(host, port, user, credential, command).await?;

        if !output.success() {
            return Err(SshError::Execution(format!(
                "Command failed with exit code {}: {}",
                output.exit_code, output.stderr
            )));
        }

        serde_json::from_str(&output.stdout).map_err(|e| {
            SshError::JsonParse(format!(
                "Failed to parse JSON output: {}, stdout: {}",
                e, output.stdout
            ))
        })
    }

    /// Write content to a file on a remote host via SFTP-like mechanism
    /// Uses a simple approach: echo content through SSH
    pub async fn write_file(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        path: &str,
        content: &str,
    ) -> SshResult<()> {
        // For sensitive paths like /etc/drbd.d/, we need sudo even for root
        // Use a multi-step approach:
        // 1. Write to temp file
        // 2. Move to target with sudo

        let temp_path = format!("/tmp/drbd-ha-{}.tmp", uuid::Uuid::new_v4());

        // Step 1: Write content to temp file (doesn't need special privileges)
        let encoded = base64_encode(content);
        let write_cmd = format!(
            "printf '%s' '{}' | base64 -d > '{}'",
            encoded.replace('"', "\""),
            temp_path.replace('"', "\"")
        );

        tracing::debug!(
            "write_file step 1: host={}, temp_path={}, content_len={}",
            host,
            temp_path,
            content.len()
        );

        let output = self
            .execute(host, port, user, credential, &write_cmd)
            .await?;
        if !output.success() {
            return Err(SshError::Execution(format!(
                "Failed to write temp file {} (exit_code={}): stderr='{}'",
                temp_path,
                output.exit_code,
                output.stderr.trim()
            )));
        }

        // Step 2: Move temp file to target location with sudo
        let move_cmd = format!(
            "mv '{}' '{}'",
            temp_path.replace('"', "\""),
            path.replace('"', "\"")
        );

        tracing::debug!(
            "write_file step 2: host={}, from={}, to={}",
            host,
            temp_path,
            path
        );

        let output = self
            .execute(host, port, user, credential, &move_cmd)
            .await?;
        if !output.success() {
            // Cleanup temp file on error
            let _ = self
                .execute(
                    host,
                    port,
                    user,
                    credential,
                    &format!("rm -f '{}'", temp_path.replace('"', "\"")),
                )
                .await;

            return Err(SshError::Execution(format!(
                "Failed to move file to {} (exit_code={}): stderr='{}'",
                path,
                output.exit_code,
                output.stderr.trim()
            )));
        }

        Ok(())
    }
    /// Read a file from a remote host
    pub async fn read_file(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        path: &str,
    ) -> SshResult<String> {
        let command = format!("cat '{}'", path.replace('"', "\""));
        let output = self.execute(host, port, user, credential, &command).await?;

        if !output.success() {
            return Err(SshError::Execution(format!(
                "Failed to read file {}: {}",
                path, output.stderr
            )));
        }

        Ok(output.stdout)
    }

    /// Check if a file exists on a remote host
    pub async fn file_exists(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        path: &str,
    ) -> SshResult<bool> {
        let command = format!("test -e '{}' && echo 'exists'", path.replace('"', "\""));
        let output = self.execute(host, port, user, credential, &command).await?;

        Ok(output.stdout.trim() == "exists")
    }

    /// Delete a file on a remote host
    pub async fn delete_file(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        path: &str,
    ) -> SshResult<()> {
        let command = format!("rm -f '{}'", path.replace('"', "\""));
        let output = self.execute(host, port, user, credential, &command).await?;

        if !output.success() {
            return Err(SshError::Execution(format!(
                "Failed to delete file {}: {}",
                path, output.stderr
            )));
        }

        Ok(())
    }
}

/// Simple base64 encoding
fn base64_encode(input: &str) -> String {
    use std::io::Write;
    let mut encoder =
        base64::write::EncoderStringWriter::new(&base64::engine::general_purpose::STANDARD);
    encoder.write_all(input.as_bytes()).unwrap();
    encoder.into_inner()
}

#[cfg(test)]

mod tests {

    use super::*;

    use crate::mock::tests::{clear_mocks, get_recorded_commands_list, push_mock_output};

    use serial_test::serial;

    #[test]

    fn test_command_output_success() {
        let output = CommandOutput {
            stdout: "hello".to_string(),

            stderr: String::new(),

            exit_code: 0,
        };

        assert!(output.success());

        let failed = CommandOutput {
            stdout: String::new(),

            stderr: "error".to_string(),

            exit_code: 1,
        };

        assert!(!failed.success());
    }

    #[test]

    fn test_session_key() {
        let key = SshManager::session_key("192.168.1.1", 22, "root");

        assert_eq!(key, "root@192.168.1.1:22");
    }

    #[tokio::test]
    #[serial]

    async fn test_execute_success() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "OK".to_string(),

            stderr: "".to_string(),

            exit_code: 0,
        });

        let config = SshConfig::default();

        let manager = SshManager::new(config);

        let result = manager
            .execute(
                "host",
                22,
                "user",
                &SshCredential::Password("pw".into()),
                "ls",
            )
            .await;

        assert!(result.is_ok());

        let output = result.unwrap();

        assert_eq!(output.stdout, "OK");

        let cmds = get_recorded_commands_list();

        assert_eq!(cmds.len(), 1);

        assert_eq!(cmds[0], "ls");
    }

    #[tokio::test]
    #[serial]

    async fn test_write_file() {
        clear_mocks();

        // step 1: write temp file

        push_mock_output(CommandOutput {
            stdout: "".into(),
            stderr: "".into(),
            exit_code: 0,
        });

        // step 2: move file

        push_mock_output(CommandOutput {
            stdout: "".into(),
            stderr: "".into(),
            exit_code: 0,
        });

        let config = SshConfig::default();

        let manager = SshManager::new(config);

        let result = manager
            .write_file(
                "host",
                22,
                "user",
                &SshCredential::Password("pw".into()),
                "/tmp/file",
                "content",
            )
            .await;

        assert!(result.is_ok());

        let cmds = get_recorded_commands_list();

        assert_eq!(cmds.len(), 2);

        assert!(cmds[0].contains("base64"));

        assert!(cmds[1].contains("mv"));
    }

    #[tokio::test]
    #[serial]

    async fn test_write_file_failure() {
        clear_mocks();

        // step 1: write temp file fails

        push_mock_output(CommandOutput {
            stdout: "".into(),
            stderr: "disk full".into(),
            exit_code: 1,
        });

        let config = SshConfig::default();

        let manager = SshManager::new(config);

        let result = manager
            .write_file(
                "host",
                22,
                "user",
                &SshCredential::Password("pw".into()),
                "/tmp/file",
                "content",
            )
            .await;

        assert!(result.is_err());

        let cmds = get_recorded_commands_list();

        assert_eq!(cmds.len(), 1);
    }

    #[tokio::test]
    #[serial]

    async fn test_read_file() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "file content".into(),

            stderr: "".into(),

            exit_code: 0,
        });

        let config = SshConfig::default();

        let manager = SshManager::new(config);

        let content = manager
            .read_file(
                "host",
                22,
                "user",
                &SshCredential::Password("pw".into()),
                "/path/to/file",
            )
            .await
            .unwrap();

        assert_eq!(content, "file content");

        let cmds = get_recorded_commands_list();

        assert_eq!(cmds.len(), 1);

        assert!(cmds[0].contains("cat '/path/to/file'"));
    }

    #[tokio::test]
    #[serial]

    async fn test_file_exists() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "exists\n".into(),

            stderr: "".into(),

            exit_code: 0,
        });

        let config = SshConfig::default();

        let manager = SshManager::new(config);

        let exists = manager
            .file_exists(
                "host",
                22,
                "user",
                &SshCredential::Password("pw".into()),
                "/path/to/file",
            )
            .await
            .unwrap();

        assert!(exists);

        let cmds = get_recorded_commands_list();

        assert_eq!(cmds.len(), 1);

        assert!(cmds[0].contains("test -e"));
    }
}

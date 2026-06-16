pub mod config;
pub mod error;
#[cfg(test)]
pub mod mock;

use crate::config::SshConfig;
use crate::error::{SshError, SshResult};

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
            // Engine: the `dispatch` library (wraps system ssh). Credential is
            // ignored — auth comes from the environment (ssh-agent / default
            // keys), same as before.

            // Connect as the configured user. If that user is not root, run
            // everything through `sudo -n` (the whole command, so pipes and
            // redirects also run as root). Root connects directly. This needs
            // passwordless sudo for non-root users.
            let sudo = user != "root";

            tracing::info!(
                "SSH execute via dispatch: host={}, port={}, user={}, sudo={}, command='{}'",
                host,
                port,
                user,
                sudo,
                command
            );

            let hr = Self::dispatch_exec(
                host,
                port,
                user,
                sudo,
                self.config.command_timeout(),
                command,
            )
            .await?;

            tracing::debug!(
                "SSH result: host={}, exit_code={}, stdout_len={}, stderr_len={}",
                host,
                hr.exit_code,
                hr.stdout.len(),
                hr.stderr.len()
            );

            Ok(CommandOutput {
                stdout: hr.stdout,
                stderr: hr.stderr,
                exit_code: hr.exit_code,
            })
        }
    }

    /// Run one command on a host through `dispatch`, replicating the previous
    /// raw-ssh behavior: `StrictHostKeyChecking=no` + `UserKnownHostsFile=/dev/null`
    /// (the cluster manages trust out-of-band and re-images nodes), no
    /// ~/.ssh/config lookups.
    #[cfg(not(test))]
    async fn dispatch_exec(
        host: &str,
        port: u16,
        user: &str,
        sudo: bool,
        timeout: std::time::Duration,
        command: &str,
    ) -> SshResult<dispatch::HostResult> {
        let cfg = dispatch::Config {
            sudo,
            ..dispatch_config::default_dispatch_config()
        };
        let client = dispatch::Dispatch::new(cfg)
            .map_err(|e| SshError::Execution(format!("dispatch init failed: {}", e)))?;
        let dest = format!("ssh://{}@{}:{}", user, host, port);
        let result = client
            .exec([dest], command)
            .timeout(timeout)
            .run()
            .await
            .map_err(|e| SshError::Execution(format!("dispatch exec failed: {}", e)))?;
        let hr = result
            .hosts
            .into_values()
            .next()
            .ok_or_else(|| SshError::Execution("dispatch returned no host result".into()))?;
        if let Some(err) = hr.error {
            return Err(SshError::Timeout(format!("{}: {}", host, err)));
        }
        Ok(hr)
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
        let safe_temp_path = escape_single_quotes(&temp_path);
        let safe_path = escape_single_quotes(path);

        // Step 1: Write content to temp file (doesn't need special privileges)
        let encoded = base64_encode(content);
        let safe_encoded = escape_single_quotes(&encoded);
        let write_cmd = format!(
            "printf '%s' '{}' | base64 -d > '{}'",
            safe_encoded, safe_temp_path
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

        // Step 2: Move temp file to target location. Ensure the destination
        // directory exists first (e.g. /etc/systemd/system/<svc>.service.d),
        // otherwise mv fails with "No such file or directory".
        let move_cmd = match std::path::Path::new(path)
            .parent()
            .and_then(|p| p.to_str())
            .filter(|p| !p.is_empty())
        {
            Some(parent) => format!(
                "mkdir -p '{}' && mv '{}' '{}'",
                escape_single_quotes(parent),
                safe_temp_path,
                safe_path
            ),
            None => format!("mv '{}' '{}'", safe_temp_path, safe_path),
        };

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
                    &format!("rm -f '{}'", safe_temp_path),
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
        let command = format!("cat '{}'", escape_single_quotes(path));
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
        let command = format!("test -e '{}' && echo 'exists'", escape_single_quotes(path));
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
        let command = format!("rm -f '{}'", escape_single_quotes(path));
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

fn escape_single_quotes(input: &str) -> String {
    input.replace('\'', "'\\''")
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

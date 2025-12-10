//! SSH connection manager
//!
//! Provides SSH connection management for executing commands on remote nodes.
//! Wrapper around ssh-cmd crate.

use crate::config::SshConfig;
use crate::core::shell_cmd::CommandOutput;
use crate::error::{AppError, AppResult};
use async_trait::async_trait;
pub use ssh_cmd::SshCredential;
use systemd_utils::{self, CommandExecutor, SystemdError, SystemdResult};

/// SSH connection manager
pub struct SshManager {
    inner: ssh_cmd::SshManager,
}

impl SshManager {
    /// Create a new SSH manager with the given configuration
    pub fn new(config: SshConfig) -> Self {
        Self {
            inner: ssh_cmd::SshManager::new(config),
        }
    }

    /// Get the session key for a host (reserved for connection pooling)
    #[allow(dead_code)]
    pub fn session_key(host: &str, port: u16, user: &str) -> String {
        ssh_cmd::SshManager::session_key(host, port, user)
    }

    /// Convert to inner SshManager (creates a new instance with same config)
    pub fn to_inner(&self) -> ssh_cmd::SshManager {
        ssh_cmd::SshManager::new(self.inner.config().clone())
    }

    /// Execute a command on a remote host
    pub async fn execute(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        command: &str,
    ) -> AppResult<CommandOutput> {
        let output = self.inner
            .execute(host, port, user, credential, command)
            .await
            .map_err(|e| AppError::Ssh(e.to_string()))?;

        Ok(CommandOutput {
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code: output.exit_code,
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
    ) -> AppResult<T> {
        self.inner
            .execute_json(host, port, user, credential, command)
            .await
            .map_err(|e| AppError::Ssh(e.to_string()))
    }

    /// Write content to a file on a remote host
    pub async fn write_file(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        path: &str,
        content: &str,
    ) -> AppResult<()> {
        self.inner
            .write_file(host, port, user, credential, path, content)
            .await
            .map_err(|e| AppError::Ssh(e.to_string()))
    }

    /// Read a file from a remote host
    pub async fn read_file(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        path: &str,
    ) -> AppResult<String> {
        self.inner
            .read_file(host, port, user, credential, path)
            .await
            .map_err(|e| AppError::Ssh(e.to_string()))
    }

    /// Check if a file exists on a remote host
    pub async fn file_exists(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        path: &str,
    ) -> AppResult<bool> {
        self.inner
            .file_exists(host, port, user, credential, path)
            .await
            .map_err(|e| AppError::Ssh(e.to_string()))
    }

    /// Delete a file on a remote host
    pub async fn delete_file(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        path: &str,
    ) -> AppResult<()> {
        self.inner
            .delete_file(host, port, user, credential, path)
            .await
            .map_err(|e| AppError::Ssh(e.to_string()))
    }
}

#[async_trait]
impl CommandExecutor<SshCredential> for SshManager {
    async fn execute(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        command: &str,
    ) -> SystemdResult<systemd_utils::CommandOutput> {
        let output = self.inner
            .execute(host, port, user, credential, command)
            .await
            .map_err(|e| SystemdError::RemoteExecution(e.to_string()))?;

        Ok(systemd_utils::CommandOutput {
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code: output.exit_code,
        })
    }
}

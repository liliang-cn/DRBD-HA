use crate::error::{SystemdError, SystemdResult};
use crate::service::ServiceStatus;
use crate::validator;
use async_trait::async_trait;
use std::marker::PhantomData;

/// Output of a command execution
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Trait for executing commands remotely
/// C: Credential type (e.g., SshCredential)
#[async_trait]
pub trait CommandExecutor<C>: Send + Sync {
    /// Execute a command on a remote host
    async fn execute(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &C,
        command: &str,
    ) -> SystemdResult<CommandOutput>;
}

/// Blanket implementation for Arc<T>
#[async_trait]
impl<C, T> CommandExecutor<C> for std::sync::Arc<T>
where
    C: Send + Sync,
    T: CommandExecutor<C> + ?Sized,
{
    async fn execute(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &C,
        command: &str,
    ) -> SystemdResult<CommandOutput> {
        (**self).execute(host, port, user, credential, command).await
    }
}

/// Remote systemd controller (uses SSH + systemctl commands)
pub struct RemoteSystemdController<C, E> {
    executor: E,
    _phantom: PhantomData<C>,
}

impl<C, E> RemoteSystemdController<C, E>
where
    C: Send + Sync,
    E: CommandExecutor<C>,
{
    pub fn new(executor: E) -> Self {
        Self {
            executor,
            _phantom: PhantomData,
        }
    }

    /// Parse systemctl show output
    fn parse_status(unit: &str, output: &str) -> ServiceStatus {
        let mut status = ServiceStatus {
            name: unit.to_string(),
            active_state: "unknown".to_string(),
            sub_state: "unknown".to_string(),
            load_state: "unknown".to_string(),
            description: String::new(),
        };

        for line in output.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "ActiveState" => status.active_state = value.to_string(),
                    "SubState" => status.sub_state = value.to_string(),
                    "LoadState" => status.load_state = value.to_string(),
                    "Description" => status.description = value.to_string(),
                    _ => {}
                }
            }
        }

        status
    }

    /// Get service status on remote node
    pub async fn status(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &C,
        unit: &str,
    ) -> SystemdResult<ServiceStatus> {
        let cmd = format!(
            "systemctl show {} --property=ActiveState,SubState,LoadState,Description --no-pager",
            unit
        );
        let output = self
            .executor
            .execute(host, port, user, credential, &cmd)
            .await?;
        Ok(Self::parse_status(unit, &output.stdout))
    }

    /// Start a service on remote node
    pub async fn start(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &C,
        unit: &str,
    ) -> SystemdResult<()> {
        validator::validate_service_name(unit)?;
        let output = self
            .executor
            .execute(
                host,
                port,
                user,
                credential,
                &format!("systemctl start {}", unit),
            )
            .await?;
        if output.success() {
            Ok(())
        } else {
            Err(SystemdError::RemoteExecution(format!(
                "Failed to start {} on {}: {}",
                unit, host, output.stderr
            )))
        }
    }

    /// Stop a service on remote node
    pub async fn stop(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &C,
        unit: &str,
    ) -> SystemdResult<()> {
        validator::validate_service_name(unit)?;
        let output = self
            .executor
            .execute(
                host,
                port,
                user,
                credential,
                &format!("systemctl stop {}", unit),
            )
            .await?;
        if output.success() {
            Ok(())
        } else {
            Err(SystemdError::RemoteExecution(format!(
                "Failed to stop {} on {}: {}",
                unit, host, output.stderr
            )))
        }
    }

    /// Restart a service on remote node
    pub async fn restart(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &C,
        unit: &str,
    ) -> SystemdResult<()> {
        validator::validate_service_name(unit)?;
        let output = self
            .executor
            .execute(
                host,
                port,
                user,
                credential,
                &format!("systemctl restart {}", unit),
            )
            .await?;
        if output.success() {
            Ok(())
        } else {
            Err(SystemdError::RemoteExecution(format!(
                "Failed to restart {} on {}: {}",
                unit, host, output.stderr
            )))
        }
    }

    /// Enable a service on remote node
    pub async fn enable(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &C,
        unit: &str,
    ) -> SystemdResult<()> {
        validator::validate_service_name(unit)?;
        let output = self
            .executor
            .execute(
                host,
                port,
                user,
                credential,
                &format!("systemctl enable {}", unit),
            )
            .await?;
        if output.success() {
            Ok(())
        } else {
            Err(SystemdError::RemoteExecution(format!(
                "Failed to enable {} on {}: {}",
                unit, host, output.stderr
            )))
        }
    }

    /// Disable a service on remote node
    pub async fn disable(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &C,
        unit: &str,
    ) -> SystemdResult<()> {
        validator::validate_service_name(unit)?;
        let output = self
            .executor
            .execute(
                host,
                port,
                user,
                credential,
                &format!("systemctl disable {}", unit),
            )
            .await?;
        if output.success() {
            Ok(())
        } else {
            Err(SystemdError::RemoteExecution(format!(
                "Failed to disable {} on {}: {}",
                unit, host, output.stderr
            )))
        }
    }

    /// Daemon reload on remote node
    pub async fn daemon_reload(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &C,
    ) -> SystemdResult<()> {
        let output = self
            .executor
            .execute(host, port, user, credential, "systemctl daemon-reload")
            .await?;
        if output.success() {
            Ok(())
        } else {
            Err(SystemdError::RemoteExecution(format!(
                "Failed to daemon-reload on {}: {}",
                host, output.stderr
            )))
        }
    }
}

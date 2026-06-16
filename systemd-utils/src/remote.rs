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
        (**self)
            .execute(host, port, user, credential, command)
            .await
    }
}

/// Parse `systemctl show` key=value output into a [`ServiceStatus`].
/// Unknown keys are ignored; missing keys keep their `"unknown"` default.
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
        Ok(parse_status(unit, &output.stdout))
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

    /// Reload a service on remote node
    pub async fn reload(
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
                &format!("systemctl reload {}", unit),
            )
            .await?;
        if output.success() {
            Ok(())
        } else {
            Err(SystemdError::RemoteExecution(format!(
                "Failed to reload {} on {}: {}",
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

    /// Disable and stop a service on remote node
    pub async fn disable_and_stop(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &C,
        unit: &str,
    ) -> SystemdResult<()> {
        self.disable(host, port, user, credential, unit).await?;
        let _ = self.stop(host, port, user, credential, unit).await;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_output_success_tracks_exit_code() {
        let ok = CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
        };
        let fail = CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 3,
        };
        assert!(ok.success());
        assert!(!fail.success());
    }

    #[test]
    fn parse_status_extracts_known_properties() {
        let output = "ActiveState=active\nSubState=running\nLoadState=loaded\nDescription=MySQL Server\nUnitFileState=enabled\n";
        let s = parse_status("mysql.service", output);
        assert_eq!(s.name, "mysql.service");
        assert_eq!(s.active_state, "active");
        assert_eq!(s.sub_state, "running");
        assert_eq!(s.load_state, "loaded");
        assert_eq!(s.description, "MySQL Server");
    }

    #[test]
    fn parse_status_defaults_for_missing_fields() {
        let s = parse_status("ghost.service", "ActiveState=inactive\n");
        assert_eq!(s.active_state, "inactive");
        // Unspecified fields keep their "unknown" defaults.
        assert_eq!(s.sub_state, "unknown");
        assert_eq!(s.load_state, "unknown");
        assert_eq!(s.description, "");
    }

    #[test]
    fn parse_status_handles_values_with_equals_signs() {
        // `Description` can legitimately contain '=' — split_once keeps the rest.
        let s = parse_status("x.service", "Description=key=value pair\n");
        assert_eq!(s.description, "key=value pair");
    }
}

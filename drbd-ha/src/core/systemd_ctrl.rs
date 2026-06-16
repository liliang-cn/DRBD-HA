//! Systemd service controller via D-Bus
//!
//! Controls systemd services using zbus D-Bus bindings.

use crate::core::{run_shell_command, SshCredential, SshManager};
use crate::error::{AppError, AppResult};
use std::sync::Arc;
pub use systemd_utils::{ServiceFileInfo, ServiceInfo, ServiceStatus, SystemdError};

enum SystemdControllerBackend {
    Local(systemd_utils::SystemdController),
    Proxy,
}

fn systemd_proxy_enabled() -> bool {
    dispatch_config::command_proxy().is_some()
}

fn parse_service_status(unit: &str, output: &str) -> ServiceStatus {
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

fn filter_system_service(name: &str, include_system: bool, prefixes: &[&str]) -> bool {
    if include_system {
        return true;
    }

    !prefixes.iter().any(|prefix| name.starts_with(prefix))
}

/// Systemd controller for local operations via D-Bus or proxied shell commands
pub struct SystemdController {
    backend: SystemdControllerBackend,
}

impl SystemdController {
    /// Create a new SystemdController with system bus connection
    pub async fn new() -> AppResult<Self> {
        if systemd_proxy_enabled() {
            Ok(Self {
                backend: SystemdControllerBackend::Proxy,
            })
        } else {
            let inner = systemd_utils::SystemdController::new()
                .await
                .map_err(|e| AppError::Systemd(e.to_string()))?;
            Ok(Self {
                backend: SystemdControllerBackend::Local(inner),
            })
        }
    }

    async fn run_proxy_systemctl(&self, command: &str, description: &str) -> AppResult<String> {
        let output = run_shell_command(command, description).await?;
        if output.success() {
            Ok(output.stdout)
        } else {
            Err(AppError::Systemd(format!(
                "{} failed: {}",
                description, output.stderr
            )))
        }
    }

    /// Get service status
    pub async fn status(&self, unit: &str) -> AppResult<ServiceStatus> {
        match &self.backend {
            SystemdControllerBackend::Local(inner) => inner
                .status(unit)
                .await
                .map_err(|e| AppError::Systemd(e.to_string())),
            SystemdControllerBackend::Proxy => {
                let output = self
                    .run_proxy_systemctl(
                        &format!(
                            "systemctl show {} --property=ActiveState,SubState,LoadState,Description --no-pager",
                            unit
                        ),
                        &format!("Get systemd status for {}", unit),
                    )
                    .await?;
                Ok(parse_service_status(unit, &output))
            }
        }
    }

    /// Start a service
    pub async fn start(&self, unit: &str) -> AppResult<()> {
        match &self.backend {
            SystemdControllerBackend::Local(inner) => inner
                .start(unit)
                .await
                .map_err(|e| AppError::Systemd(e.to_string())),
            SystemdControllerBackend::Proxy => {
                self.run_proxy_systemctl(
                    &format!("systemctl start {}", unit),
                    &format!("Start systemd unit {}", unit),
                )
                .await?;
                Ok(())
            }
        }
    }

    /// Stop a service
    pub async fn stop(&self, unit: &str) -> AppResult<()> {
        match &self.backend {
            SystemdControllerBackend::Local(inner) => inner
                .stop(unit)
                .await
                .map_err(|e| AppError::Systemd(e.to_string())),
            SystemdControllerBackend::Proxy => {
                self.run_proxy_systemctl(
                    &format!("systemctl stop {}", unit),
                    &format!("Stop systemd unit {}", unit),
                )
                .await?;
                Ok(())
            }
        }
    }

    /// Restart a service
    pub async fn restart(&self, unit: &str) -> AppResult<()> {
        match &self.backend {
            SystemdControllerBackend::Local(inner) => inner
                .restart(unit)
                .await
                .map_err(|e| AppError::Systemd(e.to_string())),
            SystemdControllerBackend::Proxy => {
                self.run_proxy_systemctl(
                    &format!("systemctl restart {}", unit),
                    &format!("Restart systemd unit {}", unit),
                )
                .await?;
                Ok(())
            }
        }
    }

    /// Reload a service
    pub async fn reload(&self, unit: &str) -> AppResult<()> {
        match &self.backend {
            SystemdControllerBackend::Local(inner) => inner
                .reload(unit)
                .await
                .map_err(|e| AppError::Systemd(e.to_string())),
            SystemdControllerBackend::Proxy => {
                self.run_proxy_systemctl(
                    &format!("systemctl reload {}", unit),
                    &format!("Reload systemd unit {}", unit),
                )
                .await?;
                Ok(())
            }
        }
    }

    /// Enable a service
    pub async fn enable(&self, unit: &str) -> AppResult<()> {
        match &self.backend {
            SystemdControllerBackend::Local(inner) => inner
                .enable(unit)
                .await
                .map_err(|e| AppError::Systemd(e.to_string())),
            SystemdControllerBackend::Proxy => {
                self.run_proxy_systemctl(
                    &format!("systemctl enable {}", unit),
                    &format!("Enable systemd unit {}", unit),
                )
                .await?;
                Ok(())
            }
        }
    }

    /// Disable a service
    pub async fn disable(&self, unit: &str) -> AppResult<()> {
        match &self.backend {
            SystemdControllerBackend::Local(inner) => inner
                .disable(unit)
                .await
                .map_err(|e| AppError::Systemd(e.to_string())),
            SystemdControllerBackend::Proxy => {
                self.run_proxy_systemctl(
                    &format!("systemctl disable {}", unit),
                    &format!("Disable systemd unit {}", unit),
                )
                .await?;
                Ok(())
            }
        }
    }

    /// Disable and stop a service (equivalent to `systemctl disable --now`)
    pub async fn disable_and_stop(&self, unit: &str) -> AppResult<()> {
        match &self.backend {
            SystemdControllerBackend::Local(inner) => inner
                .disable_and_stop(unit)
                .await
                .map_err(|e| AppError::Systemd(e.to_string())),
            SystemdControllerBackend::Proxy => {
                self.run_proxy_systemctl(
                    &format!("systemctl disable --now {}", unit),
                    &format!("Disable and stop systemd unit {}", unit),
                )
                .await?;
                Ok(())
            }
        }
    }

    /// Check if a service is enabled
    pub async fn is_enabled(&self, unit: &str) -> AppResult<bool> {
        match &self.backend {
            SystemdControllerBackend::Local(inner) => inner
                .is_enabled(unit)
                .await
                .map_err(|e| AppError::Systemd(e.to_string())),
            SystemdControllerBackend::Proxy => {
                let output = run_shell_command(
                    &format!("systemctl is-enabled {}", unit),
                    &format!("Check whether {} is enabled", unit),
                )
                .await?;
                Ok(output.success() && output.stdout.trim() == "enabled")
            }
        }
    }

    /// Reload systemd daemon
    pub async fn daemon_reload(&self) -> AppResult<()> {
        match &self.backend {
            SystemdControllerBackend::Local(inner) => inner
                .daemon_reload()
                .await
                .map_err(|e| AppError::Systemd(e.to_string())),
            SystemdControllerBackend::Proxy => {
                self.run_proxy_systemctl("systemctl daemon-reload", "Reload systemd daemon")
                    .await?;
                Ok(())
            }
        }
    }

    /// Check if drbd-reactor is running
    pub async fn is_reactor_running(&self) -> AppResult<bool> {
        let status = self.status("drbd-reactor.service").await?;
        Ok(status.is_running())
    }

    /// Reload drbd-reactor configuration
    pub async fn reload_reactor(&self) -> AppResult<()> {
        self.reload("drbd-reactor.service").await
    }

    /// Get the timestamp when a service became active (Unix timestamp in seconds)
    pub async fn get_active_enter_timestamp(&self, unit: &str) -> AppResult<i64> {
        match &self.backend {
            SystemdControllerBackend::Local(inner) => inner
                .get_active_enter_timestamp(unit)
                .await
                .map_err(|e| AppError::Systemd(e.to_string())),
            SystemdControllerBackend::Proxy => {
                let output = self
                    .run_proxy_systemctl(
                        &format!(
                            "systemctl show {} --property=ActiveEnterTimestampUSec --value",
                            unit
                        ),
                        &format!("Get activation timestamp for {}", unit),
                    )
                    .await?;
                let raw = output.trim();
                if raw.is_empty() || raw == "0" {
                    Ok(0)
                } else {
                    raw.parse::<i64>()
                        .map(|micros| micros / 1_000_000)
                        .map_err(|e| {
                            AppError::Systemd(format!(
                                "Failed to parse ActiveEnterTimestampUSec for {}: {}",
                                unit, e
                            ))
                        })
                }
            }
        }
    }

    /// List all services (units ending with .service)
    pub async fn list_services(&self, include_system: bool) -> AppResult<Vec<ServiceInfo>> {
        match &self.backend {
            SystemdControllerBackend::Local(inner) => inner
                .list_services(include_system)
                .await
                .map_err(|e| AppError::Systemd(e.to_string())),
            SystemdControllerBackend::Proxy => {
                let output = self
                    .run_proxy_systemctl(
                        "systemctl list-units --type=service --all --no-legend --no-pager --plain",
                        "List systemd services",
                    )
                    .await?;

                let system_prefixes = [
                    "systemd-",
                    "dbus",
                    "user@",
                    "getty@",
                    "serial-getty@",
                    "autovt@",
                    "console-getty",
                    "container-getty@",
                    "emergency",
                    "rescue",
                    "initrd-",
                    "kmod-static-nodes",
                    "ldconfig",
                    "modprobe@",
                    "quotaon",
                    "sys-",
                    "dev-",
                    "run-",
                    "tmp.",
                    "var-",
                    "proc-",
                    "-.mount",
                    "-.slice",
                ];

                let mut services = output
                    .lines()
                    .filter_map(|line| {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            return None;
                        }

                        let mut parts = trimmed.split_whitespace();
                        let name = parts.next()?;
                        let load_state = parts.next()?.to_string();
                        let active_state = parts.next()?.to_string();
                        let sub_state = parts.next()?.to_string();
                        let description = parts.collect::<Vec<_>>().join(" ");

                        if !name.ends_with(".service")
                            || load_state == "not-found"
                            || !filter_system_service(name, include_system, &system_prefixes)
                        {
                            return None;
                        }

                        Some(ServiceInfo {
                            name: name.to_string(),
                            description,
                            load_state,
                            active_state,
                            sub_state,
                        })
                    })
                    .collect::<Vec<_>>();

                services.sort_by(|a, b| a.name.cmp(&b.name));
                Ok(services)
            }
        }
    }

    /// List all available service unit files (including disabled ones)
    pub async fn list_service_files(
        &self,
        include_system: bool,
    ) -> AppResult<Vec<ServiceFileInfo>> {
        match &self.backend {
            SystemdControllerBackend::Local(inner) => inner
                .list_service_files(include_system)
                .await
                .map_err(|e| AppError::Systemd(e.to_string())),
            SystemdControllerBackend::Proxy => {
                let output = self
                    .run_proxy_systemctl(
                        "systemctl list-unit-files --type=service --no-legend --no-pager --plain",
                        "List systemd service files",
                    )
                    .await?;

                let system_prefixes = [
                    "systemd-",
                    "dbus",
                    "user@",
                    "getty@",
                    "serial-getty@",
                    "autovt@",
                    "console-getty",
                    "container-getty@",
                    "emergency",
                    "rescue",
                    "initrd-",
                    "kmod-static-nodes",
                    "ldconfig",
                    "modprobe@",
                    "quotaon",
                ];

                let mut services = output
                    .lines()
                    .filter_map(|line| {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            return None;
                        }

                        let mut parts = trimmed.split_whitespace();
                        let name = parts.next()?;
                        let enabled_state = parts.next()?.to_string();

                        if !name.ends_with(".service")
                            || !filter_system_service(name, include_system, &system_prefixes)
                        {
                            return None;
                        }

                        Some(ServiceFileInfo {
                            name: name.to_string(),
                            path: name.to_string(),
                            enabled_state,
                        })
                    })
                    .collect::<Vec<_>>();

                services.sort_by(|a, b| a.name.cmp(&b.name));
                Ok(services)
            }
        }
    }
}

/// Remote systemd controller (uses SSH + systemctl commands)
pub struct RemoteSystemdController {
    inner: systemd_utils::RemoteSystemdController<SshCredential, Arc<SshManager>>,
}

impl RemoteSystemdController {
    pub fn new(ssh_manager: Arc<SshManager>) -> Self {
        Self {
            inner: systemd_utils::RemoteSystemdController::new(ssh_manager),
        }
    }

    /// Get service status on remote node
    pub async fn status(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        unit: &str,
    ) -> AppResult<ServiceStatus> {
        self.inner
            .status(host, port, user, credential, unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Start a service on remote node
    pub async fn start(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        unit: &str,
    ) -> AppResult<()> {
        self.inner
            .start(host, port, user, credential, unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Stop a service on remote node
    pub async fn stop(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        unit: &str,
    ) -> AppResult<()> {
        self.inner
            .stop(host, port, user, credential, unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Restart a service on remote node
    pub async fn restart(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        unit: &str,
    ) -> AppResult<()> {
        self.inner
            .restart(host, port, user, credential, unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Reload a service on remote node
    pub async fn reload(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        unit: &str,
    ) -> AppResult<()> {
        self.inner
            .reload(host, port, user, credential, unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Enable a service on remote node
    pub async fn enable(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        unit: &str,
    ) -> AppResult<()> {
        self.inner
            .enable(host, port, user, credential, unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Disable a service on remote node
    pub async fn disable(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        unit: &str,
    ) -> AppResult<()> {
        self.inner
            .disable(host, port, user, credential, unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Disable and stop a service on remote node
    pub async fn disable_and_stop(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        unit: &str,
    ) -> AppResult<()> {
        self.inner
            .disable_and_stop(host, port, user, credential, unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Daemon reload on remote node
    pub async fn daemon_reload(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
    ) -> AppResult<()> {
        self.inner
            .daemon_reload(host, port, user, credential)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Reload drbd-reactor on remote node
    pub async fn reload_reactor(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
    ) -> AppResult<()> {
        self.reload(host, port, user, credential, "drbd-reactor.service")
            .await
    }
}

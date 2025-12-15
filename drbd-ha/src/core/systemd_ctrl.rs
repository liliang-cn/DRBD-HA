//! Systemd service controller via D-Bus
//!
//! Controls systemd services using zbus D-Bus bindings.

use crate::core::{SshCredential, SshManager};
use crate::error::{AppError, AppResult};
use std::sync::Arc;
pub use systemd_utils::{ServiceFileInfo, ServiceInfo, ServiceStatus, SystemdError};

/// Systemd controller for local operations via D-Bus
pub struct SystemdController {
    inner: systemd_utils::SystemdController,
}

impl SystemdController {
    /// Create a new SystemdController with system bus connection
    pub async fn new() -> AppResult<Self> {
        let inner = systemd_utils::SystemdController::new()
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Get service status
    pub async fn status(&self, unit: &str) -> AppResult<ServiceStatus> {
        self.inner
            .status(unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Start a service
    pub async fn start(&self, unit: &str) -> AppResult<()> {
        self.inner
            .start(unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Stop a service
    pub async fn stop(&self, unit: &str) -> AppResult<()> {
        self.inner
            .stop(unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Restart a service
    pub async fn restart(&self, unit: &str) -> AppResult<()> {
        self.inner
            .restart(unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Reload a service
    pub async fn reload(&self, unit: &str) -> AppResult<()> {
        self.inner
            .reload(unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Enable a service
    pub async fn enable(&self, unit: &str) -> AppResult<()> {
        self.inner
            .enable(unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Disable a service
    pub async fn disable(&self, unit: &str) -> AppResult<()> {
        self.inner
            .disable(unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Disable and stop a service (equivalent to `systemctl disable --now`)
    pub async fn disable_and_stop(&self, unit: &str) -> AppResult<()> {
        self.inner
            .disable_and_stop(unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Check if a service is enabled
    pub async fn is_enabled(&self, unit: &str) -> AppResult<bool> {
        self.inner
            .is_enabled(unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Reload systemd daemon
    pub async fn daemon_reload(&self) -> AppResult<()> {
        self.inner
            .daemon_reload()
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// Check if drbd-reactor is running
    pub async fn is_reactor_running(&self) -> AppResult<bool> {
        let status = self.status("drbd-reactor.service").await?;
        Ok(status.is_running())
    }

    /// Reload drbd-reactor configuration
    pub async fn reload_reactor(&self) -> AppResult<()> {
        self.restart("drbd-reactor.service").await
    }

    /// Get the timestamp when a service became active (Unix timestamp in seconds)
    pub async fn get_active_enter_timestamp(&self, unit: &str) -> AppResult<i64> {
        self.inner
            .get_active_enter_timestamp(unit)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// List all services (units ending with .service)
    pub async fn list_services(&self, include_system: bool) -> AppResult<Vec<ServiceInfo>> {
        self.inner
            .list_services(include_system)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
    }

    /// List all available service unit files (including disabled ones)
    pub async fn list_service_files(
        &self,
        include_system: bool,
    ) -> AppResult<Vec<ServiceFileInfo>> {
        self.inner
            .list_service_files(include_system)
            .await
            .map_err(|e| AppError::Systemd(e.to_string()))
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
        self.restart(host, port, user, credential, "drbd-reactor.service")
            .await
    }
}

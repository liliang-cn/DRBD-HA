//! Systemd service controller via D-Bus
//!
//! Controls systemd services using zbus D-Bus bindings.

use crate::core::validator;
use crate::core::{SshCredential, SshManager};
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use zbus::{proxy, Connection};

/// Service status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    pub active_state: String,
    pub sub_state: String,
    pub load_state: String,
    pub description: String,
}

impl ServiceStatus {
    /// Check if service is running
    pub fn is_running(&self) -> bool {
        self.active_state == "active" && self.sub_state == "running"
    }

    /// Check if service is enabled
    pub fn is_enabled(&self) -> bool {
        self.load_state == "loaded"
    }
}

/// D-Bus proxy for systemd Manager interface
#[proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait SystemdManager {
    /// Start a unit
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Stop a unit
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Restart a unit
    fn restart_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Reload a unit
    fn reload_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Get unit by name
    fn get_unit(&self, name: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Load unit
    fn load_unit(&self, name: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// List all units
    /// Returns: Vec<(name, description, load_state, active_state, sub_state, following, unit_path, job_id, job_type, job_path)>
    fn list_units(
        &self,
    ) -> zbus::Result<
        Vec<(
            String,
            String,
            String,
            String,
            String,
            String,
            zbus::zvariant::OwnedObjectPath,
            u32,
            String,
            zbus::zvariant::OwnedObjectPath,
        )>,
    >;

    /// List unit files
    /// Returns: Vec<(unit_file, state)>
    fn list_unit_files(&self) -> zbus::Result<Vec<(String, String)>>;

    /// Enable unit files
    fn enable_unit_files(
        &self,
        files: &[&str],
        runtime: bool,
        force: bool,
    ) -> zbus::Result<(bool, Vec<(String, String, String)>)>;

    /// Disable unit files
    fn disable_unit_files(
        &self,
        files: &[&str],
        runtime: bool,
    ) -> zbus::Result<Vec<(String, String, String)>>;

    /// Reload daemon
    fn reload(&self) -> zbus::Result<()>;
}

/// D-Bus proxy for systemd Unit interface
#[proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1"
)]
trait SystemdUnit {
    /// ActiveState property
    #[zbus(property)]
    fn active_state(&self) -> zbus::Result<String>;

    /// SubState property
    #[zbus(property)]
    fn sub_state(&self) -> zbus::Result<String>;

    /// LoadState property
    #[zbus(property)]
    fn load_state(&self) -> zbus::Result<String>;

    /// Description property
    #[zbus(property)]
    fn description(&self) -> zbus::Result<String>;

    /// ActiveEnterTimestamp property (microseconds since epoch)
    #[zbus(property)]
    fn active_enter_timestamp(&self) -> zbus::Result<u64>;
}

/// Systemd controller for local operations via D-Bus
pub struct SystemdController {
    connection: Connection,
}

impl SystemdController {
    /// Create a new SystemdController with system bus connection
    pub async fn new() -> AppResult<Self> {
        let connection = Connection::system()
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to connect to system bus: {}", e)))?;
        Ok(Self { connection })
    }

    /// Get the systemd manager proxy
    async fn manager(&self) -> AppResult<SystemdManagerProxy<'_>> {
        SystemdManagerProxy::new(&self.connection)
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to get manager proxy: {}", e)))
    }

    /// Get a unit proxy for the given unit path
    async fn unit_proxy(&self, path: &zbus::zvariant::OwnedObjectPath) -> AppResult<SystemdUnitProxy<'_>> {
        SystemdUnitProxy::builder(&self.connection)
            .path(path.clone())
            .map_err(|e| AppError::Systemd(format!("Failed to build unit proxy: {}", e)))?
            .build()
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to create unit proxy: {}", e)))
    }

    /// Get service status
    pub async fn status(&self, unit: &str) -> AppResult<ServiceStatus> {
        let manager = self.manager().await?;

        // Load the unit to get its path
        let unit_path = manager
            .load_unit(unit)
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to load unit {}: {}", unit, e)))?;

        let unit_proxy = self.unit_proxy(&unit_path).await?;

        let active_state = unit_proxy.active_state().await.unwrap_or_else(|_| "unknown".to_string());
        let sub_state = unit_proxy.sub_state().await.unwrap_or_else(|_| "unknown".to_string());
        let load_state = unit_proxy.load_state().await.unwrap_or_else(|_| "unknown".to_string());
        let description = unit_proxy.description().await.unwrap_or_else(|_| String::new());

        Ok(ServiceStatus {
            name: unit.to_string(),
            active_state,
            sub_state,
            load_state,
            description,
        })
    }

    /// Start a service
    pub async fn start(&self, unit: &str) -> AppResult<()> {
        validator::validate_service_name(unit)?;
        let manager = self.manager().await?;
        manager
            .start_unit(unit, "replace")
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to start {}: {}", unit, e)))?;
        Ok(())
    }

    /// Stop a service
    pub async fn stop(&self, unit: &str) -> AppResult<()> {
        validator::validate_service_name(unit)?;
        let manager = self.manager().await?;
        manager
            .stop_unit(unit, "replace")
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to stop {}: {}", unit, e)))?;
        Ok(())
    }

    /// Restart a service
    pub async fn restart(&self, unit: &str) -> AppResult<()> {
        validator::validate_service_name(unit)?;
        let manager = self.manager().await?;
        manager
            .restart_unit(unit, "replace")
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to restart {}: {}", unit, e)))?;
        Ok(())
    }

    /// Reload a service
    pub async fn reload(&self, unit: &str) -> AppResult<()> {
        validator::validate_service_name(unit)?;
        let manager = self.manager().await?;
        manager
            .reload_unit(unit, "replace")
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to reload {}: {}", unit, e)))?;
        Ok(())
    }

    /// Enable a service
    pub async fn enable(&self, unit: &str) -> AppResult<()> {
        validator::validate_service_name(unit)?;
        let manager = self.manager().await?;
        manager
            .enable_unit_files(&[unit], false, false)
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to enable {}: {}", unit, e)))?;
        Ok(())
    }

    /// Disable a service
    pub async fn disable(&self, unit: &str) -> AppResult<()> {
        validator::validate_service_name(unit)?;
        let manager = self.manager().await?;
        manager
            .disable_unit_files(&[unit], false)
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to disable {}: {}", unit, e)))?;
        Ok(())
    }

    /// Disable and stop a service (equivalent to `systemctl disable --now`)
    /// This prevents the service from starting at boot and stops it immediately
    pub async fn disable_and_stop(&self, unit: &str) -> AppResult<()> {
        // First disable to prevent auto-start at boot
        self.disable(unit).await?;
        // Then stop if running
        let _ = self.stop(unit).await; // Ignore errors if already stopped
        Ok(())
    }

    /// Check if a service is enabled
    pub async fn is_enabled(&self, unit: &str) -> AppResult<bool> {
        validator::validate_service_name(unit)?;
        let manager = self.manager().await?;
        let unit_files = manager
            .list_unit_files()
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to list unit files: {}", e)))?;

        for (path, state) in unit_files {
            if path.ends_with(unit) {
                return Ok(state == "enabled" || state == "static");
            }
        }
        Ok(false)
    }

    /// Reload systemd daemon
    pub async fn daemon_reload(&self) -> AppResult<()> {
        let manager = self.manager().await?;
        manager
            .reload()
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to daemon-reload: {}", e)))?;
        Ok(())
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
        let manager = self.manager().await?;

        // Load the unit to get its path
        let unit_path = manager
            .load_unit(unit)
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to load unit {}: {}", unit, e)))?;

        let unit_proxy = self.unit_proxy(&unit_path).await?;

        // Get the timestamp (in microseconds since epoch)
        let timestamp_usec = unit_proxy
            .active_enter_timestamp()
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to get active timestamp for {}: {}", unit, e)))?;

        // Convert to seconds
        Ok((timestamp_usec / 1_000_000) as i64)
    }

    /// List all services (units ending with .service)
    /// Returns only user/application services, filtering out system services
    pub async fn list_services(&self, include_system: bool) -> AppResult<Vec<ServiceInfo>> {
        let manager = self.manager().await?;
        let units = manager
            .list_units()
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to list units: {}", e)))?;

        // System service prefixes to filter out
        let system_prefixes = [
            "systemd-",
            "dbus",
            "user@",
            "session-",
            "init.",
            "emergency.",
            "rescue.",
            "getty@",
            "serial-getty@",
            "autovt@",
            "console-getty.",
            "container-getty@",
            "kmod-static-nodes.",
            "ldconfig.",
            "modprobe@",
            "quotaon.",
            "sys-",
            "dev-",
            "run-",
            "tmp.",
            "var-",
            "proc-",
            "-.mount",
            "-.slice",
        ];

        let mut services: Vec<ServiceInfo> = units
            .into_iter()
            .filter(|(name, _, load_state, _, _, _, _, _, _, _)| {
                // Only .service units
                if !name.ends_with(".service") {
                    return false;
                }
                // Skip not-found units
                if load_state == "not-found" {
                    return false;
                }
                // Filter system services if requested
                if !include_system {
                    for prefix in &system_prefixes {
                        if name.starts_with(prefix) {
                            return false;
                        }
                    }
                }
                true
            })
            .map(
                |(name, description, load_state, active_state, sub_state, _, _, _, _, _)| {
                    ServiceInfo {
                        name,
                        description,
                        load_state,
                        active_state,
                        sub_state,
                    }
                },
            )
            .collect();

        // Sort by name
        services.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(services)
    }

    /// List all available service unit files (including disabled ones)
    pub async fn list_service_files(&self, include_system: bool) -> AppResult<Vec<ServiceFileInfo>> {
        let manager = self.manager().await?;
        let unit_files = manager
            .list_unit_files()
            .await
            .map_err(|e| AppError::Systemd(format!("Failed to list unit files: {}", e)))?;

        // System service prefixes to filter out
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

        let mut services: Vec<ServiceFileInfo> = unit_files
            .into_iter()
            .filter(|(path, _)| {
                // Only .service files
                if !path.ends_with(".service") {
                    return false;
                }
                // Extract filename
                let filename = path.rsplit('/').next().unwrap_or(path);
                // Filter system services if requested
                if !include_system {
                    for prefix in &system_prefixes {
                        if filename.starts_with(prefix) {
                            return false;
                        }
                    }
                }
                true
            })
            .map(|(path, state)| {
                let name = path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&path)
                    .to_string();
                ServiceFileInfo {
                    name,
                    path,
                    enabled_state: state,
                }
            })
            .collect();

        // Sort by name
        services.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(services)
    }
}

/// Information about a running service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub description: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
}

/// Information about a service unit file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceFileInfo {
    pub name: String,
    pub path: String,
    pub enabled_state: String,
}

/// Remote systemd controller (uses SSH + systemctl commands)
pub struct RemoteSystemdController {
    ssh_manager: Arc<SshManager>,
}

impl RemoteSystemdController {
    pub fn new(ssh_manager: Arc<SshManager>) -> Self {
        Self { ssh_manager }
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
        credential: &SshCredential,
        unit: &str,
    ) -> AppResult<ServiceStatus> {
        let cmd = format!(
            "systemctl show {} --property=ActiveState,SubState,LoadState,Description --no-pager",
            unit
        );
        let output = self
            .ssh_manager
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
        credential: &SshCredential,
        unit: &str,
    ) -> AppResult<()> {
        validator::validate_service_name(unit)?;
        let output = self
            .ssh_manager
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
            Err(AppError::Systemd(format!(
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
        credential: &SshCredential,
        unit: &str,
    ) -> AppResult<()> {
        validator::validate_service_name(unit)?;
        let output = self
            .ssh_manager
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
            Err(AppError::Systemd(format!(
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
        credential: &SshCredential,
        unit: &str,
    ) -> AppResult<()> {
        validator::validate_service_name(unit)?;
        let output = self
            .ssh_manager
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
            Err(AppError::Systemd(format!(
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
        credential: &SshCredential,
        unit: &str,
    ) -> AppResult<()> {
        validator::validate_service_name(unit)?;
        let output = self
            .ssh_manager
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
            Err(AppError::Systemd(format!(
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
        credential: &SshCredential,
        unit: &str,
    ) -> AppResult<()> {
        validator::validate_service_name(unit)?;
        let output = self
            .ssh_manager
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
            Err(AppError::Systemd(format!(
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
        credential: &SshCredential,
    ) -> AppResult<()> {
        let output = self
            .ssh_manager
            .execute(host, port, user, credential, "systemctl daemon-reload")
            .await?;
        if output.success() {
            Ok(())
        } else {
            Err(AppError::Systemd(format!(
                "Failed to daemon-reload on {}: {}",
                host, output.stderr
            )))
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_status() {
        let output = r#"ActiveState=active
SubState=running
LoadState=loaded
Description=DRBD Reactor Daemon"#;

        let status = RemoteSystemdController::parse_status("drbd-reactor.service", output);
        assert_eq!(status.name, "drbd-reactor.service");
        assert_eq!(status.active_state, "active");
        assert_eq!(status.sub_state, "running");
        assert!(status.is_running());
    }
}

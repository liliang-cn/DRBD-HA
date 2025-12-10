use crate::error::{SystemdError, SystemdResult};
use crate::service::{ServiceFileInfo, ServiceInfo, ServiceStatus};
use crate::validator;
use zbus::{proxy, Connection};

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
    fn restart_unit(&self, name: &str, mode: &str)
        -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Reload a unit
    fn reload_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Get unit by name
    fn get_unit(&self, name: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Load unit
    fn load_unit(&self, name: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// List all units
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
    pub async fn new() -> SystemdResult<Self> {
        let connection = Connection::system()
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to connect to system bus: {}", e)))?;
        Ok(Self { connection })
    }

    /// Get the systemd manager proxy
    async fn manager(&self) -> SystemdResult<SystemdManagerProxy<'_>> {
        SystemdManagerProxy::new(&self.connection)
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to get manager proxy: {}", e)))
    }

    /// Get a unit proxy for the given unit path
    async fn unit_proxy(
        &self,
        path: &zbus::zvariant::OwnedObjectPath,
    ) -> SystemdResult<SystemdUnitProxy<'_>> {
        SystemdUnitProxy::builder(&self.connection)
            .path(path.clone())
            .map_err(|e| SystemdError::DBus(format!("Failed to build unit proxy: {}", e)))?
            .build()
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to create unit proxy: {}", e)))
    }

    /// Get service status
    pub async fn status(&self, unit: &str) -> SystemdResult<ServiceStatus> {
        let manager = self.manager().await?;

        // Load the unit to get its path
        let unit_path = manager
            .load_unit(unit)
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to load unit {}: {}", unit, e)))?;

        let unit_proxy = self.unit_proxy(&unit_path).await?;

        let active_state = unit_proxy
            .active_state()
            .await
            .unwrap_or_else(|_| "unknown".to_string());
        let sub_state = unit_proxy
            .sub_state()
            .await
            .unwrap_or_else(|_| "unknown".to_string());
        let load_state = unit_proxy
            .load_state()
            .await
            .unwrap_or_else(|_| "unknown".to_string());
        let description = unit_proxy
            .description()
            .await
            .unwrap_or_else(|_| String::new());

        Ok(ServiceStatus {
            name: unit.to_string(),
            active_state,
            sub_state,
            load_state,
            description,
        })
    }

    /// Start a service
    pub async fn start(&self, unit: &str) -> SystemdResult<()> {
        validator::validate_service_name(unit)?;
        let manager = self.manager().await?;
        manager
            .start_unit(unit, "replace")
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to start {}: {}", unit, e)))?;
        Ok(())
    }

    /// Stop a service
    pub async fn stop(&self, unit: &str) -> SystemdResult<()> {
        validator::validate_service_name(unit)?;
        let manager = self.manager().await?;
        manager
            .stop_unit(unit, "replace")
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to stop {}: {}", unit, e)))?;
        Ok(())
    }

    /// Restart a service
    pub async fn restart(&self, unit: &str) -> SystemdResult<()> {
        validator::validate_service_name(unit)?;
        let manager = self.manager().await?;
        manager
            .restart_unit(unit, "replace")
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to restart {}: {}", unit, e)))?;
        Ok(())
    }

    /// Reload a service
    pub async fn reload(&self, unit: &str) -> SystemdResult<()> {
        validator::validate_service_name(unit)?;
        let manager = self.manager().await?;
        manager
            .reload_unit(unit, "replace")
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to reload {}: {}", unit, e)))?;
        Ok(())
    }

    /// Enable a service
    pub async fn enable(&self, unit: &str) -> SystemdResult<()> {
        validator::validate_service_name(unit)?;
        let manager = self.manager().await?;
        manager
            .enable_unit_files(&[unit], false, false)
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to enable {}: {}", unit, e)))?;
        Ok(())
    }

    /// Disable a service
    pub async fn disable(&self, unit: &str) -> SystemdResult<()> {
        validator::validate_service_name(unit)?;
        let manager = self.manager().await?;
        manager
            .disable_unit_files(&[unit], false)
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to disable {}: {}", unit, e)))?;
        Ok(())
    }

    /// Disable and stop a service (equivalent to `systemctl disable --now`)
    pub async fn disable_and_stop(&self, unit: &str) -> SystemdResult<()> {
        self.disable(unit).await?;
        let _ = self.stop(unit).await;
        Ok(())
    }

    /// Check if a service is enabled
    pub async fn is_enabled(&self, unit: &str) -> SystemdResult<bool> {
        validator::validate_service_name(unit)?;
        let manager = self.manager().await?;
        let unit_files = manager
            .list_unit_files()
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to list unit files: {}", e)))?;

        for (path, state) in unit_files {
            if path.ends_with(unit) {
                return Ok(state == "enabled" || state == "static");
            }
        }
        Ok(false)
    }

    /// Reload systemd daemon
    pub async fn daemon_reload(&self) -> SystemdResult<()> {
        let manager = self.manager().await?;
        manager
            .reload()
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to daemon-reload: {}", e)))?;
        Ok(())
    }

    /// Get the timestamp when a service became active (Unix timestamp in seconds)
    pub async fn get_active_enter_timestamp(&self, unit: &str) -> SystemdResult<i64> {
        let manager = self.manager().await?;

        let unit_path = manager
            .load_unit(unit)
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to load unit {}: {}", unit, e)))?;

        let unit_proxy = self.unit_proxy(&unit_path).await?;

        let timestamp_usec = unit_proxy.active_enter_timestamp().await.map_err(|e| {
            SystemdError::DBus(format!(
                "Failed to get active timestamp for {}: {}",
                unit, e
            ))
        })?;

        Ok((timestamp_usec / 1_000_000) as i64)
    }

    /// List all services (units ending with .service)
    pub async fn list_services(&self, include_system: bool) -> SystemdResult<Vec<ServiceInfo>> {
        let manager = self.manager().await?;
        let units = manager
            .list_units()
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to list units: {}", e)))?;

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
                if !name.ends_with(".service") {
                    return false;
                }
                if load_state == "not-found" {
                    return false;
                }
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

        services.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(services)
    }

    /// List all available service unit files
    pub async fn list_service_files(
        &self,
        include_system: bool,
    ) -> SystemdResult<Vec<ServiceFileInfo>> {
        let manager = self.manager().await?;
        let unit_files = manager
            .list_unit_files()
            .await
            .map_err(|e| SystemdError::DBus(format!("Failed to list unit files: {}", e)))?;

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
                if !path.ends_with(".service") {
                    return false;
                }
                let filename = path.rsplit('/').next().unwrap_or(path);
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
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                ServiceFileInfo {
                    name,
                    path,
                    enabled_state: state,
                }
            })
            .collect();

        services.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(services)
    }
}

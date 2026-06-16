/// Systemd command builder for generating systemctl command strings
/// This is useful when you need to execute commands via SSH or build custom command chains
pub struct SystemdCmd;

impl SystemdCmd {
    /// Start a unit
    pub fn start_cmd(unit: &str) -> String {
        format!("systemctl start {}", unit)
    }

    /// Stop a unit
    pub fn stop_cmd(unit: &str) -> String {
        format!("systemctl stop {}", unit)
    }

    /// Restart a unit
    pub fn restart_cmd(unit: &str) -> String {
        format!("systemctl restart {}", unit)
    }

    /// Reload a unit
    pub fn reload_cmd(unit: &str) -> String {
        format!("systemctl reload {}", unit)
    }

    /// Enable a unit
    pub fn enable_cmd(unit: &str) -> String {
        format!("systemctl enable {}", unit)
    }

    /// Disable a unit
    pub fn disable_cmd(unit: &str) -> String {
        format!("systemctl disable {}", unit)
    }

    /// Enable and start a unit (equivalent to systemctl enable --now)
    pub fn enable_now_cmd(unit: &str) -> String {
        format!("systemctl enable --now {}", unit)
    }

    /// Disable and stop a unit (equivalent to systemctl disable --now)
    pub fn disable_now_cmd(unit: &str) -> String {
        format!("systemctl disable --now {}", unit)
    }

    /// Check if a unit is enabled
    pub fn is_enabled_cmd(unit: &str) -> String {
        format!("systemctl is-enabled {}", unit)
    }

    /// Check if a unit is active
    pub fn is_active_cmd(unit: &str) -> String {
        format!("systemctl is-active {}", unit)
    }

    /// Get unit status
    pub fn status_cmd(unit: &str) -> String {
        format!("systemctl status {}", unit)
    }

    /// Reload systemd daemon
    pub fn daemon_reload_cmd() -> String {
        "systemctl daemon-reload".to_string()
    }

    /// Reload specific service (e.g., drbd-reactor.service)
    pub fn reload_service_cmd(service: &str) -> String {
        format!("systemctl reload {}", service)
    }

    /// Show all failed units
    pub fn list_failed_cmd() -> String {
        "systemctl list-units --failed".to_string()
    }

    /// Show all units
    pub fn list_units_cmd() -> String {
        "systemctl list-units --all".to_string()
    }

    /// Show all service units
    pub fn list_services_cmd() -> String {
        "systemctl list-units --type=service --all".to_string()
    }

    /// Reset failed unit
    pub fn reset_failed_cmd(unit: Option<&str>) -> String {
        if let Some(u) = unit {
            format!("systemctl reset-failed {}", u)
        } else {
            "systemctl reset-failed".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_commands_include_unit_name() {
        assert_eq!(
            SystemdCmd::start_cmd("mysql.service"),
            "systemctl start mysql.service"
        );
        assert_eq!(
            SystemdCmd::stop_cmd("mysql.service"),
            "systemctl stop mysql.service"
        );
        assert_eq!(
            SystemdCmd::enable_now_cmd("mysql.service"),
            "systemctl enable --now mysql.service"
        );
        assert_eq!(
            SystemdCmd::disable_now_cmd("mysql.service"),
            "systemctl disable --now mysql.service"
        );
        assert_eq!(
            SystemdCmd::is_enabled_cmd("mysql.service"),
            "systemctl is-enabled mysql.service"
        );
    }

    #[test]
    fn reset_failed_handles_optional_unit() {
        assert_eq!(SystemdCmd::reset_failed_cmd(None), "systemctl reset-failed");
        assert_eq!(
            SystemdCmd::reset_failed_cmd(Some("r0.service")),
            "systemctl reset-failed r0.service"
        );
    }

    #[test]
    fn global_commands_have_no_unit() {
        assert_eq!(SystemdCmd::daemon_reload_cmd(), "systemctl daemon-reload");
        assert_eq!(
            SystemdCmd::list_services_cmd(),
            "systemctl list-units --type=service --all"
        );
    }
}

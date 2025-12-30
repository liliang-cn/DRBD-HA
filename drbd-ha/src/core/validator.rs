//! Input validation module
//!
//! Provides validation functions to prevent command injection and other security issues.
#![allow(clippy::incompatible_msrv)]

use crate::error::{AppError, AppResult};
use crate::models::OcfAgentConfig;
use drbd_utils::get_used_minors_from_config;
use regex::Regex;
use std::sync::LazyLock;

// Pre-compiled regex patterns
static RESOURCE_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]{0,63}$").unwrap());

static BLOCK_DEVICE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^/dev/(sd[a-z]+\d*|nvme\d+n\d+(p\d+)?|vd[a-z]+\d*|drbd\d+|loop\d+)$").unwrap()
});

static IP_ADDRESS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{1,3}\.){3}\d{1,3}$").unwrap());

static HOSTNAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z][a-zA-Z0-9-]{0,62}$").unwrap());

static MOUNT_POINT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/[a-zA-Z0-9/_-]+$").unwrap());

static SERVICE_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| {
        // Support all systemd unit types: service, target, socket, device, mount, automount,
        // swap, timer, path, slice, scope
        // Note: systemd unit names can contain letters, digits, :, -, _, ., and @
        // They can also contain * for device units (for glob patterns)
        Regex::new(r"^[a-zA-Z0-9_:.@*\-]+\.(service|target|socket|device|mount|automount|swap|timer|path|slice|scope)$").unwrap()
    });

/// Validate DRBD resource name
pub fn validate_resource_name(name: &str) -> AppResult<()> {
    if name.is_empty() {
        return Err(AppError::Validation(
            "Resource name cannot be empty".to_string(),
        ));
    }
    if !RESOURCE_NAME_RE.is_match(name) {
        return Err(AppError::Validation(format!(
            "Invalid resource name '{}'. Must start with a letter, contain only alphanumeric, underscore, or hyphen, max 64 chars",
            name
        )));
    }
    Ok(())
}

/// Validate block device path
pub fn validate_block_device(path: &str) -> AppResult<()> {
    if !BLOCK_DEVICE_RE.is_match(path) {
        return Err(AppError::Validation(format!(
            "Invalid block device path '{}'. Must be /dev/sdX, /dev/nvmeXnY, /dev/vdX, /dev/drbdN, or /dev/loopN",
            path
        )));
    }
    Ok(())
}

/// Validate IP address
pub fn validate_ip_address(ip: &str) -> AppResult<()> {
    if !IP_ADDRESS_RE.is_match(ip) {
        return Err(AppError::Validation(format!("Invalid IP address '{}'", ip)));
    }
    // Additional check for valid octets
    for octet in ip.split('.') {
        let num: u32 = octet
            .parse()
            .map_err(|_| AppError::Validation(format!("Invalid IP address '{}'", ip)))?;
        if num > 255 {
            return Err(AppError::Validation(format!(
                "Invalid IP address '{}': octet {} > 255",
                ip, num
            )));
        }
    }
    Ok(())
}

/// Validate hostname
pub fn validate_hostname(hostname: &str) -> AppResult<()> {
    if hostname.is_empty() {
        return Err(AppError::Validation("Hostname cannot be empty".to_string()));
    }
    if !HOSTNAME_RE.is_match(hostname) {
        return Err(AppError::Validation(format!(
            "Invalid hostname '{}'. Must start with a letter, contain only alphanumeric or hyphen",
            hostname
        )));
    }
    Ok(())
}

/// Validate mount point path
pub fn validate_mount_point(path: &str) -> AppResult<()> {
    if !path.starts_with('/') {
        return Err(AppError::Validation(format!(
            "Mount point '{}' must be an absolute path",
            path
        )));
    }
    if !MOUNT_POINT_RE.is_match(path) {
        return Err(AppError::Validation(format!(
            "Invalid mount point '{}'. Must be an absolute path with alphanumeric, underscore, or hyphen",
            path
        )));
    }
    // Prevent path traversal
    if path.contains("..") {
        return Err(AppError::Validation(format!(
            "Mount point '{}' cannot contain '..'",
            path
        )));
    }
    Ok(())
}

/// Validate systemd unit name
pub fn validate_service_name(name: &str) -> AppResult<()> {
    if !SERVICE_NAME_RE.is_match(name) {
        return Err(AppError::Validation(format!(
            "Invalid systemd unit name '{}'. Must end with a valid unit type (.service, .target, .socket, etc.)",
            name
        )));
    }
    Ok(())
}

/// Validate DRBD port number
pub fn validate_port(port: u16) -> AppResult<()> {
    if !(7000..=8000).contains(&port) {
        return Err(AppError::Validation(format!(
            "DRBD port {} should be between 7000 and 8000",
            port
        )));
    }
    Ok(())
}

/// Validate DRBD minor number
/// Validates that the minor number is in a valid range (0-1048575)
pub fn validate_minor(minor: u32) -> AppResult<()> {
    // DRBD supports minor numbers from 0 to 1048575
    const MAX_MINOR: u32 = 1048575;

    if minor > MAX_MINOR {
        return Err(AppError::Validation(format!(
            "DRBD minor {} is invalid. Maximum minor number is {}",
            minor, MAX_MINOR
        )));
    }
    Ok(())
}

/// Check if a minor number is already in use by existing resources
pub fn validate_minor_available(minor: u32) -> AppResult<()> {
    let used_minors = get_used_minors_from_config();

    if used_minors.contains(&minor) {
        return Err(AppError::Validation(format!(
            "DRBD minor {} is already in use by another resource",
            minor
        )));
    }
    Ok(())
}

/// Check if DRBD device name conflicts with existing resources
pub async fn validate_device_unique(device_name: &str, config_dir: &str) -> AppResult<()> {
    // Check against existing DRBD configuration files
    let config_dir = config_dir;

    // List all .res files
    match tokio::fs::read_dir(config_dir).await {
        Ok(mut entries) => {
            while let Ok(Some(entry)) = entries.next_entry().await {
                // Only process .res files
                let file_name_os = entry.file_name();
                let file_name = match file_name_os.to_str() {
                    Some(name) => name,
                    None => continue,
                };

                if !file_name.ends_with(".res") {
                    continue;
                }

                let file_path = entry.path();

                // Read the content of the .res file
                match tokio::fs::read_to_string(&file_path).await {
                    Ok(content) => {
                        // Check if this file contains our target device name
                        for line in content.lines() {
                            let trimmed_line = line.trim();
                            // Look for device lines like: device /dev/drbd1234;
                            if trimmed_line.starts_with("device ") {
                                let device_part = trimmed_line.strip_prefix("device ")
                                    .unwrap_or("")
                                    .trim_end_matches(';');

                                if device_part == device_name {
                                    let resource_name = file_name
                                        .strip_suffix(".res")
                                        .unwrap_or("unknown");

                                    return Err(AppError::Validation(format!(
                                        "DRBD device '{}' is already used by resource '{}'",
                                        device_name, resource_name
                                    )));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read DRBD config file '{:?}': {}", file_path, e);
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to read DRBD config directory '{}': {}", config_dir, e);
        }
    }

    // Also check against active DRBD status
    let cmd = crate::core::drbd_cmd::DrbdCmd::status_cmd();

    match crate::core::run_shell_command(&cmd, "Check active DRBD resources").await {
        Ok(output) => {
            if output.success() {
                // Parse DRBD status to find device conflicts
                for line in output.stdout.lines() {
                    // Look for device paths in status output
                    if line.contains(device_name) {
                        return Err(AppError::Validation(format!(
                            "DRBD device '{}' is currently active and in use",
                            device_name
                        )));
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to check DRBD status for device conflicts: {}", e);
        }
    }

    Ok(())
}

/// Validate VIP CIDR netmask
pub fn validate_netmask(netmask: u8) -> AppResult<()> {
    if netmask == 0 || netmask > 32 {
        return Err(AppError::Validation(format!(
            "Invalid netmask /{}. Must be between 1 and 32",
            netmask
        )));
    }
    Ok(())
}

/// Result of disk safety checks
#[derive(Debug)]
pub struct DiskSafetyCheck {
    pub device: String,
    pub is_safe: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl DiskSafetyCheck {
    pub fn new(device: &str) -> Self {
        Self {
            device: device.to_string(),
            is_safe: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn add_warning(&mut self, msg: String) {
        self.warnings.push(msg);
    }

    pub fn add_error(&mut self, msg: String) {
        self.is_safe = false;
        self.errors.push(msg);
    }
}

/// Validate filesystem type
pub fn validate_fs_type(fs_type: &str) -> AppResult<()> {
    match fs_type {
        "xfs" | "ext4" | "btrfs" => Ok(()),
        _ => Err(AppError::Validation(format!(
            "Unsupported filesystem type: '{}'. Supported types are xfs, ext4, btrfs",
            fs_type
        ))),
    }
}

/// Check if a device is currently mounted
pub fn check_device_mounted(device: &str, mount_output: &str) -> bool {
    // Check /proc/mounts format: device mountpoint fstype options
    for line in mount_output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[0] == device {
            return true;
        }
    }
    false
}

/// Check if a device has a filesystem signature
pub fn check_device_has_filesystem(blkid_output: &str) -> bool {
    // blkid returns empty or error for devices without filesystem
    !blkid_output.trim().is_empty()
}

/// Check if a device is used by DRBD (is a backing device)
pub fn check_device_used_by_drbd(device: &str, drbd_status: &str) -> bool {
    // Simple check: if device path appears in any DRBD config/status
    drbd_status.contains(device)
}

/// Check if a device is a DRBD device (not a backing device)
pub fn is_drbd_device(device: &str) -> bool {
    device.starts_with("/dev/drbd")
}

/// Check if a device is in use by the system (LVM, MD, etc.)
pub fn check_device_in_use(holders_output: &str) -> bool {
    // /sys/block/sdX/holders/ will have entries if device is in use
    !holders_output.trim().is_empty()
}

/// Validate OCF agents configuration
pub fn validate_ocf_agents(agents: &[OcfAgentConfig]) -> AppResult<()> {
    if agents.is_empty() {
        return Ok(());
    }

    let ocf_root = std::env::var("OCF_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/usr/lib/ocf"));

    for agent_config in agents {
        let parts: Vec<&str> = agent_config.name.split(':').collect();
        // Expect ocf:provider:agent
        if parts.len() != 3 || parts[0] != "ocf" {
            return Err(AppError::Validation(format!(
                "Invalid OCF agent name '{}'. Expected format: ocf:provider:agent",
                agent_config.name
            )));
        }
        let provider = parts[1];
        let agent_name = parts[2];

        let agent_path = ocf_root
            .join("resource.d")
            .join(provider)
            .join(agent_name);
        if !agent_path.exists() {
            return Err(AppError::Validation(format!(
                "OCF agent '{}' not found at {:?}",
                agent_config.name, agent_path
            )));
        }

        match ra_params::get_agent_metadata(&agent_path) {
            Ok((metadata, _)) => {
                for param in metadata.parameters.parameters {
                    if (param.required == "1" || param.required == "true")
                        && param.content.default.is_empty()
                    {
                        // It is required and has no default value.
                        // Must be present in agent_config.params
                        if !agent_config.params.contains_key(&param.name) {
                            return Err(AppError::Validation(format!(
                                "Missing required parameter '{}' for agent '{}'",
                                param.name, agent_config.name
                            )));
                        }
                    }
                }
            }
            Err(e) => {
                return Err(AppError::Validation(format!(
                    "Failed to get metadata for agent '{}': {}",
                    agent_config.name, e
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_resource_name() {
        assert!(validate_resource_name("r0").is_ok());
        assert!(validate_resource_name("my_resource").is_ok());
        assert!(validate_resource_name("my-resource-1").is_ok());
        assert!(validate_resource_name("").is_err());
        assert!(validate_resource_name("0invalid").is_err());
        assert!(validate_resource_name("has space").is_err());
        assert!(validate_resource_name("has;semicolon").is_err());
    }

    #[test]
    fn test_validate_block_device() {
        assert!(validate_block_device("/dev/sda").is_ok());
        assert!(validate_block_device("/dev/sdb1").is_ok());
        assert!(validate_block_device("/dev/nvme0n1").is_ok());
        assert!(validate_block_device("/dev/nvme0n1p1").is_ok());
        assert!(validate_block_device("/dev/vda").is_ok());
        assert!(validate_block_device("/dev/drbd0").is_ok());
        assert!(validate_block_device("/dev/loop0").is_ok());
        assert!(validate_block_device("/etc/passwd").is_err());
        assert!(validate_block_device("sda").is_err());
    }

    #[test]
    fn test_validate_ip_address() {
        assert!(validate_ip_address("192.168.1.1").is_ok());
        assert!(validate_ip_address("10.0.0.1").is_ok());
        assert!(validate_ip_address("256.1.1.1").is_err());
        assert!(validate_ip_address("1.1.1").is_err());
        assert!(validate_ip_address("not-an-ip").is_err());
    }

    #[test]
    fn test_validate_mount_point() {
        assert!(validate_mount_point("/mnt/data").is_ok());
        assert!(validate_mount_point("/var/lib/mysql").is_ok());
        assert!(validate_mount_point("relative/path").is_err());
        assert!(validate_mount_point("/path/../escape").is_err());
    }

    #[test]
    fn test_validate_service_name() {
        // Test valid service units
        assert!(validate_service_name("nginx.service").is_ok());
        assert!(validate_service_name("my-app.service").is_ok());
        assert!(validate_service_name("drbd-promote@r0.service").is_ok());

        // Test other valid systemd unit types
        assert!(validate_service_name("network-online.target").is_ok());
        assert!(validate_service_name("rpcbind.socket").is_ok());
        assert!(validate_service_name("sys-devices-pci*.device").is_ok());
        assert!(validate_service_name("data.mount").is_ok());
        assert!(validate_service_name("home.automount").is_ok());
        assert!(validate_service_name("swapfile.swap").is_ok());
        assert!(validate_service_name("backup.timer").is_ok());
        assert!(validate_service_name("monitoring.path").is_ok());
        assert!(validate_service_name("system.slice").is_ok());

        // Test invalid names
        assert!(validate_service_name("nginx").is_err()); // missing unit type
        assert!(validate_service_name("bad;name.service").is_err()); // invalid character
        assert!(validate_service_name("service.invalid").is_err()); // unsupported unit type
    }

    #[test]
    fn test_check_device_mounted() {
        let mount_output = "/dev/sda1 / ext4 rw,relatime 0 0\n/dev/sdb1 /mnt/data xfs rw 0 0";
        assert!(check_device_mounted("/dev/sda1", mount_output));
        assert!(check_device_mounted("/dev/sdb1", mount_output));
        assert!(!check_device_mounted("/dev/sdc1", mount_output));
        assert!(!check_device_mounted("/dev/drbd0", mount_output));
    }

    #[test]
    fn test_check_device_has_filesystem() {
        assert!(check_device_has_filesystem(
            "/dev/sda1: UUID=\"abc\" TYPE=\"ext4\""
        ));
        assert!(!check_device_has_filesystem(""));
        assert!(!check_device_has_filesystem("   "));
    }

    #[test]
    fn test_is_drbd_device() {
        assert!(is_drbd_device("/dev/drbd0"));
        assert!(is_drbd_device("/dev/drbd123"));
        assert!(!is_drbd_device("/dev/sda"));
        assert!(!is_drbd_device("/dev/nvme0n1"));
    }

    #[test]
    fn test_disk_safety_check() {
        let mut check = DiskSafetyCheck::new("/dev/sda");
        assert!(check.is_safe);
        assert!(check.errors.is_empty());

        check.add_warning("Device has existing data".to_string());
        assert!(check.is_safe);
        assert_eq!(check.warnings.len(), 1);

        check.add_error("Device is mounted".to_string());
        assert!(!check.is_safe);
        assert_eq!(check.errors.len(), 1);
    }
}

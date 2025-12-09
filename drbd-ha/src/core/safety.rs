//! Safety checks for disk operations and network connectivity
//!
//! Provides comprehensive safety validation before critical operations
//! like mkfs, mount, and multi-node resource creation.

use crate::core::{run_shell_command, SshCredential, SshManager};
use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::sync::Arc;
use tracing::{info, warn};

/// Result of safety checks
#[derive(Debug, Clone, Serialize)]
pub struct SafetyCheckResult {
    /// Whether it's safe to proceed
    pub is_safe: bool,
    /// Warning messages (non-blocking)
    pub warnings: Vec<String>,
    /// Error messages (blocking)
    pub errors: Vec<String>,
    /// Device or target being checked
    pub target: String,
}

impl SafetyCheckResult {
    pub fn new(target: &str) -> Self {
        Self {
            is_safe: true,
            warnings: Vec::new(),
            errors: Vec::new(),
            target: target.to_string(),
        }
    }

    pub fn add_warning(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }

    pub fn add_error(&mut self, msg: impl Into<String>) {
        self.is_safe = false;
        self.errors.push(msg.into());
    }

    /// Convert to AppError if not safe
    pub fn to_error(&self) -> Option<AppError> {
        if self.is_safe {
            None
        } else {
            Some(AppError::Validation(format!(
                "Safety check failed for {}: {}",
                self.target,
                self.errors.join("; ")
            )))
        }
    }
}

/// Network connectivity check result
#[derive(Debug, Clone, Serialize)]
pub struct ConnectivityCheckResult {
    pub host: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// Safety checker for disk and network operations
pub struct SafetyChecker {
    ssh_manager: Arc<SshManager>,
}

impl SafetyChecker {
    pub fn new(ssh_manager: Arc<SshManager>) -> Self {
        Self { ssh_manager }
    }

    /// Check if a device is safe for mkfs operation
    /// This prevents accidentally formatting the wrong disk
    pub async fn check_device_for_mkfs(&self, device: &str) -> AppResult<SafetyCheckResult> {
        let mut result = SafetyCheckResult::new(device);

        info!("Running safety checks for mkfs on {}", device);

        // 1. Check if device exists
        let exists_cmd = format!("test -b {} && echo exists", device);
        let exists_output = run_shell_command(&exists_cmd, &format!("Check if device {} exists", device)).await?;
        if !exists_output.stdout.contains("exists") {
            result.add_error(format!("Device {} does not exist or is not a block device", device));
            return Ok(result);
        }

        // 2. Check if device is mounted
        let mount_output = run_shell_command("cat /proc/mounts", "Read /proc/mounts to check if device is mounted").await?;
        if mount_output.stdout.contains(device) {
            result.add_error(format!("Device {} is currently mounted", device));
        }

        // 3. Check if device has existing filesystem
        let blkid_cmd = format!("blkid {} 2>/dev/null || true", device);
        let blkid_output = run_shell_command(&blkid_cmd, &format!("Check for existing filesystem on {}", device)).await?;
        if !blkid_output.stdout.trim().is_empty() {
            // Extract filesystem type if present
            if blkid_output.stdout.contains("TYPE=") {
                result.add_warning(format!(
                    "Device {} has existing filesystem signature. Data will be DESTROYED!",
                    device
                ));
            }
        }

        // 4. Check if device is a system disk (contains root filesystem)
        let root_disk = self.get_root_disk().await?;
        if let Some(root) = root_disk {
            if device.starts_with(&root) {
                result.add_error(format!(
                    "Device {} appears to be the system disk (root: {}). Refusing to format!",
                    device, root
                ));
            }
        }

        // 5. Check if device is in use by LVM/MD/etc
        let device_name = device.trim_start_matches("/dev/");
        let holders_cmd = format!("ls /sys/block/{}/holders/ 2>/dev/null || true", device_name);
        let holders_output = run_shell_command(&holders_cmd, &format!("Check if device {} is in use by LVM/MD/etc", device)).await?;
        if !holders_output.stdout.trim().is_empty() {
            result.add_error(format!(
                "Device {} is in use by: {}",
                device,
                holders_output.stdout.trim()
            ));
        }

        // 6. For non-DRBD devices, warn about direct formatting
        if !device.starts_with("/dev/drbd") {
            result.add_warning(format!(
                "Device {} is not a DRBD device. Consider using it as a DRBD backing device instead.",
                device
            ));
        }

        if result.is_safe {
            info!("Device {} passed all safety checks", device);
        } else {
            warn!("Device {} failed safety checks: {:?}", device, result.errors);
        }

        Ok(result)
    }

    /// Check if a backing device is safe to use for DRBD
    pub async fn check_device_for_drbd(&self, device: &str) -> AppResult<SafetyCheckResult> {
        let mut result = SafetyCheckResult::new(device);

        info!("Running safety checks for DRBD backing device {}", device);

        // 1. Check if device exists
        let exists_cmd = format!("test -b {} && echo exists", device);
        let exists_output = run_shell_command(&exists_cmd, &format!("Check if device {} exists for DRBD", device)).await?;
        if !exists_output.stdout.contains("exists") {
            result.add_error(format!("Device {} does not exist or is not a block device", device));
            return Ok(result);
        }

        // 2. Check if device is already in use as a DRBD backing device
        // Check all DRBD resource configs for this device
        let drbdadm_dump = run_shell_command("drbdadm dump all 2>/dev/null || true", "Dump DRBD configuration to check for existing usage").await?;
        if drbdadm_dump.stdout.contains(device) {
            result.add_error(format!(
                "Device {} is already configured as a DRBD backing device",
                device
            ));
        }

        // 3. Check if device is mounted
        let mount_output = run_shell_command("cat /proc/mounts", "Read /proc/mounts to check if device is mounted").await?;
        if mount_output.stdout.contains(device) {
            result.add_error(format!("Device {} is currently mounted", device));
        }

        // 4. Check if device is a system disk
        let root_disk = self.get_root_disk().await?;
        if let Some(root) = root_disk {
            if device.starts_with(&root) {
                result.add_error(format!(
                    "Device {} appears to be the system disk. Refusing to use as DRBD backing device!",
                    device
                ));
            }
        }

        // 5. Warn about existing data
        let blkid_cmd = format!("blkid {} 2>/dev/null || true", device);
        let blkid_output = run_shell_command(&blkid_cmd, &format!("Check for existing data on {}", device)).await?;
        if !blkid_output.stdout.trim().is_empty() {
            result.add_warning(format!(
                "Device {} has existing data. It will be overwritten by DRBD metadata!",
                device
            ));
        }

        if result.is_safe {
            info!("Device {} passed DRBD backing device checks", device);
        } else {
            warn!("Device {} failed DRBD checks: {:?}", device, result.errors);
        }

        Ok(result)
    }

    /// Check network connectivity to multiple nodes
    pub async fn check_network_connectivity(
        &self,
        nodes: &[(String, u16, String, SshCredential)], // (host, port, user, credential)
    ) -> AppResult<Vec<ConnectivityCheckResult>> {
        let mut results = Vec::new();

        for (host, port, user, credential) in nodes {
            info!("Checking connectivity to {}:{}", host, port);

            let start = std::time::Instant::now();
            let check_result = self
                .ssh_manager
                .execute(host, *port, user, credential, "echo ok")
                .await;
            let latency = start.elapsed().as_millis() as u64;

            let result = match check_result {
                Ok(output) if output.success() => ConnectivityCheckResult {
                    host: host.clone(),
                    reachable: true,
                    latency_ms: Some(latency),
                    error: None,
                },
                Ok(output) => ConnectivityCheckResult {
                    host: host.clone(),
                    reachable: false,
                    latency_ms: Some(latency),
                    error: Some(format!("Command failed: {}", output.stderr)),
                },
                Err(e) => ConnectivityCheckResult {
                    host: host.clone(),
                    reachable: false,
                    latency_ms: None,
                    error: Some(e.to_string()),
                },
            };

            if !result.reachable {
                warn!("Node {} is not reachable: {:?}", host, result.error);
            }

            results.push(result);
        }

        Ok(results)
    }

    /// Verify all nodes are reachable before multi-node operation
    pub async fn verify_all_nodes_reachable(
        &self,
        nodes: &[(String, u16, String, SshCredential)],
    ) -> AppResult<()> {
        let results = self.check_network_connectivity(nodes).await?;

        let unreachable: Vec<_> = results.iter().filter(|r| !r.reachable).collect();

        if !unreachable.is_empty() {
            let msg = unreachable
                .iter()
                .map(|r| format!("{}: {}", r.host, r.error.as_deref().unwrap_or("unknown error")))
                .collect::<Vec<_>>()
                .join("; ");

            return Err(AppError::Network(format!(
                "Cannot proceed: {} node(s) unreachable: {}",
                unreachable.len(),
                msg
            )));
        }

        info!("All {} nodes are reachable", nodes.len());
        Ok(())
    }

    /// Get the disk device containing the root filesystem
    async fn get_root_disk(&self) -> AppResult<Option<String>> {
        // Find the device for root filesystem
        let output = run_shell_command("df / | tail -1 | awk '{print $1}'", "Get root filesystem device").await?;
        let root_device = output.stdout.trim();

        if root_device.is_empty() {
            return Ok(None);
        }

        // Strip partition number to get base disk
        // e.g., /dev/sda1 -> /dev/sda, /dev/nvme0n1p1 -> /dev/nvme0n1
        let base_disk = if root_device.contains("nvme") {
            // NVMe: /dev/nvme0n1p1 -> /dev/nvme0n1
            root_device.trim_end_matches(|c: char| c == 'p' || c.is_ascii_digit())
                .trim_end_matches('p')
                .to_string()
        } else {
            // Regular: /dev/sda1 -> /dev/sda
            root_device.trim_end_matches(|c: char| c.is_ascii_digit()).to_string()
        };

        Ok(Some(base_disk))
    }

    /// Check remote device safety (for remote nodes)
    pub async fn check_remote_device_for_drbd(
        &self,
        host: &str,
        port: u16,
        user: &str,
        credential: &SshCredential,
        device: &str,
    ) -> AppResult<SafetyCheckResult> {
        let mut result = SafetyCheckResult::new(&format!("{}:{}", host, device));

        info!("Running remote safety checks on {} for device {}", host, device);

        // 1. Check if device exists
        let exists_cmd = format!("test -b {} && echo exists", device);
        let exists_output = self.ssh_manager.execute(host, port, user, credential, &exists_cmd).await?;
        if !exists_output.stdout.contains("exists") {
            result.add_error(format!("Device {} does not exist on {}", device, host));
            return Ok(result);
        }

        // 2. Check if device is mounted
        let mount_cmd = "cat /proc/mounts";
        let mount_output = self.ssh_manager.execute(host, port, user, credential, mount_cmd).await?;
        if mount_output.stdout.contains(device) {
            result.add_error(format!("Device {} is currently mounted on {}", device, host));
        }

        // 3. Check if already used by DRBD
        let drbd_cmd = "drbdadm dump all 2>/dev/null || true";
        let drbd_output = self.ssh_manager.execute(host, port, user, credential, drbd_cmd).await?;
        if drbd_output.stdout.contains(device) {
            result.add_error(format!(
                "Device {} is already configured as DRBD backing device on {}",
                device, host
            ));
        }

        if result.is_safe {
            info!("Remote device {} on {} passed safety checks", device, host);
        } else {
            warn!("Remote device {} on {} failed: {:?}", device, host, result.errors);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safety_check_result() {
        let mut result = SafetyCheckResult::new("/dev/sda");
        assert!(result.is_safe);
        assert!(result.to_error().is_none());

        result.add_warning("Test warning");
        assert!(result.is_safe);
        assert_eq!(result.warnings.len(), 1);

        result.add_error("Test error");
        assert!(!result.is_safe);
        assert!(result.to_error().is_some());
    }

    #[test]
    fn test_connectivity_check_result() {
        let result = ConnectivityCheckResult {
            host: "192.168.1.1".to_string(),
            reachable: true,
            latency_ms: Some(50),
            error: None,
        };
        assert!(result.reachable);
        assert_eq!(result.latency_ms, Some(50));
    }
}

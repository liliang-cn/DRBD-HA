//! DRBD command wrapper
//!
//! Wraps drbdadm and drbdsetup commands for resource management.
//! Delegates to drbd-utils crate.

use crate::error::{AppError, AppResult};

pub use drbd_utils::{
    ConnectionStatus, DeviceStatus, DrbdStatus, PeerDeviceStatus, ResourceStatus,
};

/// DRBD command builder wrapper
pub struct DrbdCmd;

impl DrbdCmd {
    pub fn status_cmd() -> String {
        drbd_utils::DrbdCmd::status_cmd()
    }

    pub fn resource_status_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::resource_status_cmd(resource)
            .map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn create_md_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::create_md_cmd(resource)
            .map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn adjust_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::adjust_cmd(resource).map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn up_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::up_cmd(resource).map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn down_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::down_cmd(resource).map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn primary_cmd(resource: &str, force: bool) -> AppResult<String> {
        drbd_utils::DrbdCmd::primary_cmd(resource, force)
            .map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn secondary_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::secondary_cmd(resource)
            .map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn connect_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::connect_cmd(resource).map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn disconnect_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::disconnect_cmd(resource)
            .map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn invalidate_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::invalidate_cmd(resource)
            .map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn verify_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::verify_cmd(resource).map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn connect_discard_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::connect_discard_cmd(resource)
            .map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn pause_sync_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::pause_sync_cmd(resource)
            .map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn resume_sync_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::resume_sync_cmd(resource)
            .map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn resize_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::resize_cmd(resource).map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn dump_cmd(resource: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::dump_cmd(resource).map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn check_config_cmd() -> String {
        drbd_utils::DrbdCmd::check_config_cmd()
    }

    pub fn version_cmd() -> String {
        drbd_utils::DrbdCmd::version_cmd()
    }

    pub fn load_module_cmd() -> String {
        drbd_utils::DrbdCmd::load_module_cmd()
    }

    pub fn mkfs_cmd(device: &str, fstype: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::mkfs_cmd(device, fstype)
            .map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn mount_cmd(device: &str, mount_point: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::mount_cmd(device, mount_point)
            .map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn umount_cmd(mount_point: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::umount_cmd(mount_point)
            .map_err(|e| AppError::Validation(e.to_string()))
    }

    pub fn mkdir_cmd(mount_point: &str) -> AppResult<String> {
        drbd_utils::DrbdCmd::mkdir_cmd(mount_point).map_err(|e| AppError::Validation(e.to_string()))
    }
}

pub fn parse_drbd_status(json_output: &str) -> AppResult<Vec<ResourceStatus>> {
    drbd_utils::parse_drbd_status(json_output).map_err(|e| AppError::Drbd(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drbd_commands() {
        assert!(DrbdCmd::up_cmd("r0").is_ok());
        assert!(DrbdCmd::up_cmd("my-resource").is_ok());
        assert!(DrbdCmd::up_cmd("bad;name").is_err());
        assert!(DrbdCmd::up_cmd("").is_err());
    }

    #[test]
    fn test_mkfs_validation() {
        assert!(DrbdCmd::mkfs_cmd("/dev/drbd0", "ext4").is_ok());
        assert!(DrbdCmd::mkfs_cmd("/dev/drbd0", "xfs").is_ok());
        assert!(DrbdCmd::mkfs_cmd("/dev/drbd0", "ntfs").is_err());
        assert!(DrbdCmd::mkfs_cmd("/etc/passwd", "ext4").is_err());
    }

    #[test]
    fn test_parse_drbd_status() {
        let json = r#"[{"name":"r0","role":"Primary","devices":[{"volume":0,"disk-state":"UpToDate","minor":0}],"connections":[]}]"#;
        let status = parse_drbd_status(json).unwrap();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].name, "r0");
        assert!(status[0].is_primary());
        assert!(status[0].is_uptodate());
    }

    #[test]
    fn test_resource_status_methods() {
        let status = ResourceStatus {
            name: "r0".to_string(),
            role: "Primary".to_string(),
            devices: vec![DeviceStatus {
                volume: 0,
                disk_state: "UpToDate".to_string(),
                minor: 0,
                size: Some(1073741824),
            }],
            connections: vec![ConnectionStatus {
                peer_node_id: 1,
                name: "node2".to_string(),
                connection_state: "Connected".to_string(),
                peer_role: Some("Secondary".to_string()),
                peer_devices: vec![PeerDeviceStatus {
                    volume: 0,
                    replication_state: "Established".to_string(),
                    peer_disk_state: "UpToDate".to_string(),
                    percent_in_sync: Some(100.0),
                }],
            }],
        };

        assert!(status.is_primary());
        assert!(status.is_connected());
        assert!(status.is_uptodate());
        assert!(!status.is_syncing());
        assert!(status.sync_progress().is_none());
    }
}

pub mod cmd;
pub mod error;
pub mod models;
pub mod parser;
pub mod remote;
pub mod sync;
pub mod validator;
pub mod verification;

// Re-export configuration types from config-gen (single source of truth)
pub use config_gen::{ConfigGenerator, ConfigPaths, NodeConfig, ResourceConfig};

pub use cmd::{parse_drbd_status, DrbdCmd};
pub use error::{DrbdError, DrbdResult};
pub use models::{
    ConnectionStatus, DeviceStatus, DrbdPeerStatus, DrbdResourceStatus, DrbdStatus,
    PeerDeviceStatus, ResourceStatus,
};
pub use parser::{
    allocate_minor, convert_resource_status, get_used_minors_from_config, parse_drbdadm_status,
    parse_res_file_for_minors, parse_res_file_for_nodes,
};
pub use remote::{resolve_hostname_to_ip, CommandOutput, RemoteDrbdQuery, RemoteExecutor};
pub use sync::{check_drbd_sync_complete, get_drbd_sync_status, PeerSyncStatus, SyncStatus};
pub use verification::{DrbdVerifier, VerificationConfig, VerificationDetails, VerificationResult};

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

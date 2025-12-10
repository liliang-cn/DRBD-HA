use crate::error::{LvmError, LvmResult};
use serde::Deserialize;
use ssh_cmd::{CommandOutput, SshCredential, SshManager};
use std::sync::Arc;

#[cfg(not(test))]
use crate::cmd::run_local_command;

/// Represents LVM Volume Group information
#[derive(Debug, Clone)]
pub struct LvmVgInfo {
    pub name: String,
    pub size: u64,     // Total size in bytes
    pub free: u64,     // Free size in bytes
    pub pv_count: u32, // Number of Physical Volumes
    pub lv_count: u32, // Number of Logical Volumes
}

/// Client for querying LVM information (local or remote)
pub struct LvmClient {
    #[allow(dead_code)] // Field is used in non-test builds, but triggered as unused in tests
    ssh: Option<(Arc<SshManager>, String, u16, String, SshCredential)>,
}

impl LvmClient {
    /// Create a new local LVM client
    pub fn new_local() -> Self {
        Self { ssh: None }
    }

    /// Create a new remote LVM client
    pub fn new_remote(
        ssh_manager: Arc<SshManager>,
        host: String,
        port: u16,
        user: String,
        credential: SshCredential,
    ) -> Self {
        Self {
            ssh: Some((ssh_manager, host, port, user, credential)),
        }
    }

    /// Execute command locally or remotely
    async fn execute(&self, command: &str, description: &str) -> LvmResult<CommandOutput> {
        #[cfg(test)]
        {
            // Avoid unused variable warning in test mode
            let _ = description;
            
            crate::mock::tests::record_command(command.to_string());
            if let Some(output) = crate::mock::tests::get_next_mock_output() {
                return Ok(output);
            }
            panic!("Test tried to execute command without mock output: {}", command);
        }

        #[cfg(not(test))]
        if let Some((manager, host, port, user, credential)) = &self.ssh {
            manager
                .execute(host, *port, user, credential, command)
                .await
                .map_err(LvmError::Ssh)
        } else {
            run_local_command(command, description).await
        }
    }

    /// Get info about a specific LVM Volume Group using JSON output
    pub async fn get_vg_info(&self, vg_name: &str) -> LvmResult<Option<LvmVgInfo>> {
        let cmd = format!(
            "vgs --reportformat json --units b --nosuffix -o vg_name,vg_size,vg_free,pv_count,lv_count {}",
            vg_name
        );
        let output = self.execute(&cmd, "Get LVM VG info").await?;

        if !output.success() {
            if output.stderr.contains("not found") {
                return Ok(None);
            }
            return Err(LvmError::Execution(output.stderr));
        }

        let vgs = parse_vgs_json(&output.stdout)?;
        Ok(vgs.into_iter().find(|v| v.name == vg_name))
    }

    /// List all LVM Volume Groups using JSON output
    pub async fn list_vg_info(&self) -> LvmResult<Vec<LvmVgInfo>> {
        let cmd = "vgs --reportformat json --units b --nosuffix -o vg_name,vg_size,vg_free,pv_count,lv_count";
        let output = self.execute(cmd, "List all LVM VGs").await?;

        if !output.success() {
            return Err(LvmError::Execution(output.stderr));
        }

        parse_vgs_json(&output.stdout)
    }

    /// Initialize LVM Volume Group on a disk
    pub async fn init_pool(&self, vg_name: &str, disk: &str) -> LvmResult<()> {
        let cmd = format!("vgcreate {} {}", vg_name, disk);
        let output = self
            .execute(&cmd, &format!("Initialize LVM volume group '{}' on disk '{}'", vg_name, disk))
            .await?;

        if !output.success() {
            return Err(LvmError::Execution(format!("Failed to initialize LVM pool: {}", output.stderr)));
        }
        Ok(())
    }

    /// Create LVM Logical Volume
    pub async fn create_volume(&self, vg_name: &str, vol_name: &str, size_gb: u64) -> LvmResult<String> {
        let cmd = format!(
            "lvcreate -L {}G -n {} {} --yes",
            size_gb, vol_name, vg_name
        );
        let output = self
            .execute(&cmd, &format!("Create LVM logical volume '{}' of {}GB in VG '{}'", vol_name, size_gb, vg_name))
            .await?;

        if !output.success() {
            return Err(LvmError::Execution(format!("Failed to create LVM volume: {}", output.stderr)));
        }
        Ok(format!("/dev/{}/{}", vg_name, vol_name))
    }

    /// Delete LVM Logical Volume
    pub async fn delete_volume(&self, vg_name: &str, vol_name: &str) -> LvmResult<()> {
        let cmd = format!("lvremove -f /dev/{}/{}", vg_name, vol_name);
        let output = self
            .execute(&cmd, &format!("Remove LVM logical volume '{}' from VG '{}'", vol_name, vg_name))
            .await?;

        if !output.success() {
            return Err(LvmError::Execution(format!("Failed to delete LVM volume: {}", output.stderr)));
        }
        Ok(())
    }

    /// Resize LVM Logical Volume
    pub async fn resize_volume(&self, vg_name: &str, vol_name: &str, new_size_gb: u64) -> LvmResult<()> {
        let cmd = format!(
            "lvextend -L {}G /dev/{}/{}",
            new_size_gb, vg_name, vol_name
        );
        let output = self
            .execute(&cmd, &format!("Resize LVM logical volume '{}' to {}GB in VG '{}'", vol_name, new_size_gb, vg_name))
            .await?;

        if !output.success() {
            return Err(LvmError::Execution(format!("Failed to resize LVM volume: {}", output.stderr)));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct VgsReport {
    report: Vec<VgsReportItem>,
}

#[derive(Debug, Deserialize)]
struct VgsReportItem {
    vg: Vec<VgsEntry>,
}

#[derive(Debug, Deserialize)]
struct VgsEntry {
    vg_name: String,
    vg_size: String,
    vg_free: String,
    pv_count: String,
    lv_count: String,
}

fn parse_vgs_json(json_str: &str) -> LvmResult<Vec<LvmVgInfo>> {
    let report: VgsReport = serde_json::from_str(json_str)
        .map_err(|e| LvmError::JsonParse(format!("Failed to parse vgs JSON: {}", e)))?;

    let mut vgs = Vec::new();
    for item in report.report {
        for vg in item.vg {
            vgs.push(LvmVgInfo {
                name: vg.vg_name,
                size: vg.vg_size.parse().unwrap_or(0),
                free: vg.vg_free.parse().unwrap_or(0),
                pv_count: vg.pv_count.parse().unwrap_or(0),
                lv_count: vg.lv_count.parse().unwrap_or(0),
            });
        }
    }
    Ok(vgs)
}

/// Get LVM Volume Group information by name (Local).
/// Wraps `LvmClient::new_local().get_vg_info(vg_name)`
pub async fn get_vg_info(vg_name: &str) -> LvmResult<Option<LvmVgInfo>> {
    LvmClient::new_local().get_vg_info(vg_name).await
}

/// List all LVM Volume Groups (Local).
/// Wraps `LvmClient::new_local().list_vg_info()`
pub async fn list_vg_info() -> LvmResult<Vec<LvmVgInfo>> {
    LvmClient::new_local().list_vg_info().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::tests::{clear_mocks, push_mock_output, get_recorded_commands_list};
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_get_vg_info_parsing() {
        clear_mocks();
        
        let json_output = r#"{
            "report": [
                {
                    "vg": [
                        {
                            "vg_name": "test_vg",
                            "vg_size": "10737418240",
                            "vg_free": "5368709120",
                            "pv_count": "1",
                            "lv_count": "2"
                        }
                    ]
                }
            ]
        }"#;

        push_mock_output(CommandOutput {
            stdout: json_output.to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        });

        let client = LvmClient::new_local();
        let vg_info = client.get_vg_info("test_vg").await.unwrap();

        assert!(vg_info.is_some());
        let info = vg_info.unwrap();
        assert_eq!(info.name, "test_vg");
        assert_eq!(info.size, 10737418240);
        assert_eq!(info.free, 5368709120);
        assert_eq!(info.pv_count, 1);
        assert_eq!(info.lv_count, 2);

        let cmds = get_recorded_commands_list();
        assert_eq!(cmds.len(), 1);
        assert!(cmds[0].contains("vgs"));
        assert!(cmds[0].contains("test_vg"));
    }

    #[tokio::test]
    #[serial]
    async fn test_get_vg_info_not_found() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "".to_string(),
            stderr: "Volume group \"nonexistent\" not found".to_string(),
            exit_code: 5, // LVM often returns 5 for not found
        });

        let client = LvmClient::new_local();
        let vg_info = client.get_vg_info("nonexistent").await.unwrap();

        assert!(vg_info.is_none());
    }

    #[tokio::test]
    #[serial]
    async fn test_init_pool() {
        clear_mocks();
        
        push_mock_output(CommandOutput {
            stdout: "Volume group \"new_vg\" successfully created".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        });

        let client = LvmClient::new_local();
        let result = client.init_pool("new_vg", "/dev/sdb").await;

        assert!(result.is_ok());
        
        let cmds = get_recorded_commands_list();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "vgcreate new_vg /dev/sdb");
    }

    #[tokio::test]
    #[serial]
    async fn test_create_volume() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "Logical volume \"vol1\" created.".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        });

        let client = LvmClient::new_local();
        let path = client.create_volume("myvg", "vol1", 10).await.unwrap();

        assert_eq!(path, "/dev/myvg/vol1");

        let cmds = get_recorded_commands_list();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "lvcreate -L 10G -n vol1 myvg --yes");
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_volume() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "Logical volume \"vol1\" successfully removed".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        });

        let client = LvmClient::new_local();
        let result = client.delete_volume("myvg", "vol1").await;

        assert!(result.is_ok());

        let cmds = get_recorded_commands_list();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "lvremove -f /dev/myvg/vol1");
    }

    #[tokio::test]
    #[serial]
    async fn test_resize_volume() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "Size of logical volume myvg/vol1 changed from 10.00 GiB (2560 extents) to 20.00 GiB (5120 extents).".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        });

        let client = LvmClient::new_local();
        let result = client.resize_volume("myvg", "vol1", 20).await;

        assert!(result.is_ok());

        let cmds = get_recorded_commands_list();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "lvextend -L 20G /dev/myvg/vol1");
    }

    #[tokio::test]
    #[serial]
    async fn test_parse_vgs_error() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "invalid json".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        });

        let client = LvmClient::new_local();
        let result = client.list_vg_info().await;

        assert!(result.is_err());
        match result.unwrap_err() {
            LvmError::JsonParse(_) => {},
            _ => panic!("Expected JsonParse error"),
        }
    }
}

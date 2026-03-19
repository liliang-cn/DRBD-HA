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

/// Represents LVM Logical Volume information
#[derive(Debug, Clone)]
pub struct LvmLvInfo {
    pub name: String,
    pub vg_name: String,
    pub size: u64,
    pub attr: String,
    pub pool_lv: String,
    pub origin: String,
    pub data_percent: String,
    pub metadata_percent: String,
    pub move_pv: String,
    pub mirror_log: String,
    pub copy_percent: String,
    pub convert_lv: String,
}

/// Represents LVM Thin Pool information
#[derive(Debug, Clone)]
pub struct LvmThinPoolInfo {
    pub name: String,
    pub vg_name: String,
    pub size: u64,
    pub data_percent: String,
    pub metadata_percent: String,
    pub transaction_id: String,
    pub zeroing: bool,
}

/// Represents LVM Thin Volume information
#[derive(Debug, Clone)]
pub struct LvmThinVolumeInfo {
    pub name: String,
    pub vg_name: String,
    pub pool_name: String,
    pub size: u64,
    pub data_percent: String,
    pub origin: String,
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
            panic!(
                "Test tried to execute command without mock output: {}",
                command
            );
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

    /// List all LVM Logical Volumes using JSON output
    pub async fn list_lvs(&self) -> LvmResult<Vec<LvmLvInfo>> {
        let cmd = "lvs --reportformat json --units b --nosuffix -o lv_name,vg_name,lv_size,lv_attr,pool_lv,origin,data_percent,metadata_percent,move_pv,mirror_log,copy_percent,convert_lv";
        let output = self.execute(cmd, "List all LVM LVs").await?;

        if !output.success() {
            return Err(LvmError::Execution(output.stderr));
        }

        parse_lvs_json(&output.stdout)
    }

    /// List available (unused) LVM Logical Volumes
    ///
    /// Filters LVs that are not open and are of a usable type (linear or thin volume).
    pub async fn list_available_lvs(&self) -> LvmResult<Vec<LvmLvInfo>> {
        let lvs = self.list_lvs().await?;

        let available_lvs: Vec<LvmLvInfo> = lvs
            .into_iter()
            .filter(|lv| {
                // attr index 5 is Open status ('o' means open)
                // Example attr: "-wi-a-----"
                // Indices:
                // 0: Volume type
                // 1: Permissions
                // 2: Allocation policy
                // 3: Fixed (minor) number
                // 4: State
                // 5: Open status (o = open)

                // Safety check for attr length
                let is_open = if lv.attr.len() > 5 {
                    lv.attr.chars().nth(5) == Some('o')
                } else {
                    // If attr is weirdly short, assume it might be open/unsafe?
                    true
                };

                // Also skip if it is a snapshot or thin pool data etc
                // type (index 0): 's' snapshot, 't' thin pool, 'T' thin pool data, 'v' virtual, 'V' thin volume
                // We only want standard volumes ('-') or thin volumes ('V')
                let type_char = lv.attr.chars().next().unwrap_or('-');
                let is_usable_type = type_char == '-' || type_char == 'V';

                !is_open && is_usable_type
            })
            .collect();

        Ok(available_lvs)
    }

    /// Initialize LVM Volume Group on a disk
    pub async fn init_pool(&self, vg_name: &str, disk: &str) -> LvmResult<()> {
        // First initialize the physical volume
        // We use -y -ff to force initialization even if it looks like it has a partition table
        let pv_cmd = format!("pvcreate -y -ff {}", disk);
        let pv_output = self
            .execute(
                &pv_cmd,
                &format!("Initialize physical volume on '{}'", disk),
            )
            .await?;

        if !pv_output.success() {
            return Err(LvmError::Execution(format!(
                "Failed to initialize physical volume: {}",
                pv_output.stderr
            )));
        }

        // Then create the volume group
        let cmd = format!("vgcreate -y -ff {} {}", vg_name, disk);
        let output = self
            .execute(
                &cmd,
                &format!(
                    "Initialize LVM volume group '{}' on disk '{}'",
                    vg_name, disk
                ),
            )
            .await?;

        if !output.success() {
            return Err(LvmError::Execution(format!(
                "Failed to initialize LVM pool: {}",
                output.stderr
            )));
        }
        Ok(())
    }

    /// Create LVM Logical Volume
    pub async fn create_volume(
        &self,
        vg_name: &str,
        vol_name: &str,
        size_gb: u64,
    ) -> LvmResult<String> {
        let cmd = format!("lvcreate -L {}G -n {} {} --yes", size_gb, vol_name, vg_name);
        let output = self
            .execute(
                &cmd,
                &format!(
                    "Create LVM logical volume '{}' of {}GB in VG '{}'",
                    vol_name, size_gb, vg_name
                ),
            )
            .await?;

        if !output.success() {
            return Err(LvmError::Execution(format!(
                "Failed to create LVM volume: {}",
                output.stderr
            )));
        }
        Ok(format!("/dev/{}/{}", vg_name, vol_name))
    }

    /// Delete LVM Logical Volume
    pub async fn delete_volume(&self, vg_name: &str, vol_name: &str) -> LvmResult<()> {
        let cmd = format!("lvremove -f /dev/{}/{}", vg_name, vol_name);
        let output = self
            .execute(
                &cmd,
                &format!(
                    "Remove LVM logical volume '{}' from VG '{}'",
                    vol_name, vg_name
                ),
            )
            .await?;

        if !output.success() {
            return Err(LvmError::Execution(format!(
                "Failed to delete LVM volume: {}",
                output.stderr
            )));
        }
        Ok(())
    }

    /// Resize LVM Logical Volume
    pub async fn resize_volume(
        &self,
        vg_name: &str,
        vol_name: &str,
        new_size_gb: u64,
    ) -> LvmResult<()> {
        let cmd = format!("lvextend -L {}G /dev/{}/{}", new_size_gb, vg_name, vol_name);
        let output = self
            .execute(
                &cmd,
                &format!(
                    "Resize LVM logical volume '{}' to {}GB in VG '{}'",
                    vol_name, new_size_gb, vg_name
                ),
            )
            .await?;

        if !output.success() {
            return Err(LvmError::Execution(format!(
                "Failed to resize LVM volume: {}",
                output.stderr
            )));
        }
        Ok(())
    }

    /// Create LVM Thin Pool
    pub async fn create_thin_pool(
        &self,
        vg_name: &str,
        pool_name: &str,
        size_gb: u64,
    ) -> LvmResult<()> {
        let cmd = format!(
            "lvcreate --type thin-pool -L {}G -n {} {}",
            size_gb, pool_name, vg_name
        );
        let output = self
            .execute(
                &cmd,
                &format!(
                    "Create LVM thin pool '{}' of {}GB in VG '{}'",
                    pool_name, size_gb, vg_name
                ),
            )
            .await?;

        if !output.success() {
            return Err(LvmError::Execution(format!(
                "Failed to create LVM thin pool: {}",
                output.stderr
            )));
        }
        Ok(())
    }

    /// Create LVM Thin Volume in a Thin Pool
    pub async fn create_thin_volume(
        &self,
        vg_name: &str,
        pool_name: &str,
        volume_name: &str,
        size_gb: u64,
    ) -> LvmResult<String> {
        let cmd = format!(
            "lvcreate --type thin -V {}G -n {} {}/{}",
            size_gb, volume_name, vg_name, pool_name
        );
        let output = self
            .execute(
                &cmd,
                &format!(
                    "Create LVM thin volume '{}' of {}GB in pool '{}/{}'",
                    volume_name, size_gb, vg_name, pool_name
                ),
            )
            .await?;

        if !output.success() {
            return Err(LvmError::Execution(format!(
                "Failed to create LVM thin volume: {}",
                output.stderr
            )));
        }
        Ok(format!("/dev/{}/{}", vg_name, volume_name))
    }

    /// Get information about a specific LVM Thin Pool
    pub async fn get_thin_pool_info(
        &self,
        vg_name: &str,
        pool_name: &str,
    ) -> LvmResult<Option<LvmThinPoolInfo>> {
        let cmd = format!(
            "lvs --reportformat json --units b --nosuffix -o lv_name,vg_name,lv_size,data_percent,metadata_percent,lv_tags {}/{}",
            vg_name, pool_name
        );
        let output = self.execute(&cmd, "Get LVM thin pool info").await?;

        if !output.success() {
            if output.stderr.contains("not found") {
                return Ok(None);
            }
            return Err(LvmError::Execution(output.stderr));
        }

        let pools = parse_thin_pools_json(&output.stdout)?;
        Ok(pools
            .into_iter()
            .find(|p| p.name == pool_name && p.vg_name == vg_name))
    }

    /// List all LVM Thin Pools
    pub async fn list_thin_pools(&self, vg_name: Option<&str>) -> LvmResult<Vec<LvmThinPoolInfo>> {
        let cmd = if let Some(vg) = vg_name {
            format!("lvs --reportformat json --units b --nosuffix -o lv_name,vg_name,lv_size,data_percent,metadata_percent,lv_tags {}", vg)
        } else {
            "lvs --reportformat json --units b --nosuffix -o lv_name,vg_name,lv_size,data_percent,metadata_percent,lv_tags".to_string()
        };

        let output = self.execute(&cmd, "List LVM thin pools").await?;

        if !output.success() {
            return Err(LvmError::Execution(output.stderr));
        }

        parse_thin_pools_json(&output.stdout)
    }

    /// List LVM Thin Volumes
    pub async fn list_thin_volumes(
        &self,
        pool_name: Option<&str>,
    ) -> LvmResult<Vec<LvmThinVolumeInfo>> {
        let cmd = "lvs --reportformat json --units b --nosuffix -o lv_name,vg_name,pool_lv,lv_size,data_percent,origin";
        let output = self.execute(cmd, "List LVM thin volumes").await?;

        if !output.success() {
            return Err(LvmError::Execution(output.stderr));
        }

        let volumes = parse_thin_volumes_json(&output.stdout)?;
        if let Some(pool) = pool_name {
            Ok(volumes
                .into_iter()
                .filter(|v| v.pool_name == pool)
                .collect())
        } else {
            Ok(volumes)
        }
    }

    /// Convert existing LV to Thin Pool
    pub async fn convert_to_thin_pool(&self, vg_name: &str, lv_name: &str) -> LvmResult<()> {
        let cmd = format!("lvconvert --type thin-pool {}/{}", vg_name, lv_name);
        let output = self
            .execute(
                &cmd,
                &format!("Convert LV '{}' to thin pool in VG '{}'", lv_name, vg_name),
            )
            .await?;

        if !output.success() {
            return Err(LvmError::Execution(format!(
                "Failed to convert LV to thin pool: {}",
                output.stderr
            )));
        }
        Ok(())
    }

    /// Delete Thin Volume
    pub async fn delete_thin_volume(&self, vg_name: &str, volume_name: &str) -> LvmResult<()> {
        let cmd = format!("lvremove -f {}/{}", vg_name, volume_name);
        let output = self
            .execute(
                &cmd,
                &format!("Delete thin volume '{}' from VG '{}'", volume_name, vg_name),
            )
            .await?;

        if !output.success() {
            return Err(LvmError::Execution(format!(
                "Failed to delete thin volume: {}",
                output.stderr
            )));
        }
        Ok(())
    }

    /// Create snapshot of Thin Volume
    pub async fn create_thin_snapshot(
        &self,
        vg_name: &str,
        source_volume: &str,
        snapshot_name: &str,
    ) -> LvmResult<String> {
        let cmd = format!(
            "lvcreate -s -n {} {}/{}",
            snapshot_name, vg_name, source_volume
        );
        let output = self
            .execute(
                &cmd,
                &format!(
                    "Create thin snapshot '{}' of volume '{}/{}'",
                    snapshot_name, vg_name, source_volume
                ),
            )
            .await?;

        if !output.success() {
            return Err(LvmError::Execution(format!(
                "Failed to create thin snapshot: {}",
                output.stderr
            )));
        }
        Ok(format!("/dev/{}/{}", vg_name, snapshot_name))
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

#[derive(Debug, Deserialize)]
struct LvsReport {
    report: Vec<LvsReportItem>,
}

#[derive(Debug, Deserialize)]
struct LvsReportItem {
    lv: Vec<LvsEntry>,
}

#[derive(Debug, Deserialize)]
struct LvsEntry {
    lv_name: String,
    vg_name: String,
    lv_size: String,
    lv_attr: String,
    pool_lv: String,
    origin: String,
    data_percent: String,
    metadata_percent: String,
    move_pv: String,
    mirror_log: String,
    copy_percent: String,
    convert_lv: String,
}

fn parse_lvs_json(json_str: &str) -> LvmResult<Vec<LvmLvInfo>> {
    let report: LvsReport = serde_json::from_str(json_str)
        .map_err(|e| LvmError::JsonParse(format!("Failed to parse lvs JSON: {}", e)))?;

    let mut lvs = Vec::new();
    for item in report.report {
        for lv in item.lv {
            lvs.push(LvmLvInfo {
                name: lv.lv_name,
                vg_name: lv.vg_name,
                size: lv.lv_size.parse().unwrap_or(0),
                attr: lv.lv_attr,
                pool_lv: lv.pool_lv,
                origin: lv.origin,
                data_percent: lv.data_percent,
                metadata_percent: lv.metadata_percent,
                move_pv: lv.move_pv,
                mirror_log: lv.mirror_log,
                copy_percent: lv.copy_percent,
                convert_lv: lv.convert_lv,
            });
        }
    }
    Ok(lvs)
}

// Thin pool parsing functions
fn parse_thin_pools_json(json_str: &str) -> LvmResult<Vec<LvmThinPoolInfo>> {
    let report: LvsReport = serde_json::from_str(json_str)
        .map_err(|e| LvmError::JsonParse(format!("Failed to parse thin pools JSON: {}", e)))?;

    let mut pools = Vec::new();
    for item in report.report {
        for lv in item.lv {
            // Check if this is a thin pool by looking at the attributes
            if lv.lv_attr.len() >= 2 && lv.lv_attr.chars().nth(1) == Some('t') {
                pools.push(LvmThinPoolInfo {
                    name: lv.lv_name,
                    vg_name: lv.vg_name,
                    size: lv.lv_size.parse().unwrap_or(0),
                    data_percent: lv.data_percent,
                    metadata_percent: lv.metadata_percent,
                    transaction_id: "0".to_string(), // This would need additional parsing from lv_tags
                    zeroing: true,                   // Default assumption
                });
            }
        }
    }
    Ok(pools)
}

fn parse_thin_volumes_json(json_str: &str) -> LvmResult<Vec<LvmThinVolumeInfo>> {
    let report: LvsReport = serde_json::from_str(json_str)
        .map_err(|e| LvmError::JsonParse(format!("Failed to parse thin volumes JSON: {}", e)))?;

    let mut volumes = Vec::new();
    for item in report.report {
        for lv in item.lv {
            // Check if this is a thin volume by looking at the attributes
            if lv.lv_attr.len() >= 2 && lv.lv_attr.chars().nth(1) == Some('V') {
                volumes.push(LvmThinVolumeInfo {
                    name: lv.lv_name,
                    vg_name: lv.vg_name,
                    pool_name: lv.pool_lv,
                    size: lv.lv_size.parse().unwrap_or(0),
                    data_percent: lv.data_percent,
                    origin: lv.origin,
                });
            }
        }
    }
    Ok(volumes)
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

/// List all LVM Logical Volumes (Local).
/// Wraps `LvmClient::new_local().list_lvs()`
pub async fn list_lvs() -> LvmResult<Vec<LvmLvInfo>> {
    LvmClient::new_local().list_lvs().await
}

/// Get LVM Thin Pool information by name (Local).
/// Wraps `LvmClient::new_local().get_thin_pool_info(vg_name, pool_name)`
pub async fn get_thin_pool_info(
    vg_name: &str,
    pool_name: &str,
) -> LvmResult<Option<LvmThinPoolInfo>> {
    LvmClient::new_local()
        .get_thin_pool_info(vg_name, pool_name)
        .await
}

/// List all LVM Thin Pools (Local).
/// Wraps `LvmClient::new_local().list_thin_pools()`
pub async fn list_thin_pools() -> LvmResult<Vec<LvmThinPoolInfo>> {
    LvmClient::new_local().list_thin_pools(None).await
}

/// List all LVM Thin Volumes (Local).
/// Wraps `LvmClient::new_local().list_thin_volumes()`
pub async fn list_thin_volumes() -> LvmResult<Vec<LvmThinVolumeInfo>> {
    LvmClient::new_local().list_thin_volumes(None).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::tests::{clear_mocks, get_recorded_commands_list, push_mock_output};
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
            LvmError::JsonParse(_) => {}
            _ => panic!("Expected JsonParse error"),
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_create_thin_pool() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "Logical volume \"thinpool\" created.".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        });

        let client = LvmClient::new_local();
        let result = client.create_thin_pool("myvg", "thinpool", 100).await;

        assert!(result.is_ok());

        let cmds = get_recorded_commands_list();
        assert_eq!(cmds.len(), 1);
        assert_eq!(
            cmds[0],
            "lvcreate --type thin-pool -L 100G -n thinpool myvg"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_create_thin_volume() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "Logical volume \"thinvol\" created.".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        });

        let client = LvmClient::new_local();
        let result = client
            .create_thin_volume("myvg", "thinpool", "thinvol", 20)
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "/dev/myvg/thinvol");

        let cmds = get_recorded_commands_list();
        assert_eq!(cmds.len(), 1);
        assert_eq!(
            cmds[0],
            "lvcreate --type thin -V 20G -n thinvol myvg/thinpool"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_create_thin_snapshot() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "Logical volume \"thinvol_snap\" created.".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        });

        let client = LvmClient::new_local();
        let result = client
            .create_thin_snapshot("myvg", "thinvol", "thinvol_snap")
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "/dev/myvg/thinvol_snap");

        let cmds = get_recorded_commands_list();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "lvcreate -s -n thinvol_snap myvg/thinvol");
    }
}

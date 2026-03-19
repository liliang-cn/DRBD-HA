use crate::error::{LvmError, LvmResult};
use ssh_cmd::CommandOutput;
use tokio::process::Command;
use tracing::{debug, error, info};

/// LVM command builder for generating LVM command strings
/// This is useful when you need to execute commands via SSH or build custom command chains
pub struct LvmCmd;

impl LvmCmd {
    /// Create physical volume
    pub fn pvcreate_cmd(disk: &str) -> String {
        format!("pvcreate -y -ff {}", disk)
    }

    /// Create volume group
    pub fn vgcreate_cmd(vg_name: &str, disk: &str) -> String {
        format!("vgcreate -y -ff {} {}", vg_name, disk)
    }

    /// Create thin pool
    pub fn create_thin_pool_cmd(vg_name: &str, pool_name: &str, size: &str) -> String {
        format!(
            "lvcreate -y -L {} --type thin -n {} {}",
            size, pool_name, vg_name
        )
    }

    /// Create thin volume from thin pool
    pub fn create_thin_volume_cmd(
        vg_name: &str,
        pool_name: &str,
        vol_name: &str,
        size: &str,
    ) -> String {
        format!(
            "lvcreate -y -V {} --type thin -n {} --thinpool {} {}",
            size, vol_name, pool_name, vg_name
        )
    }

    /// Create regular logical volume
    pub fn create_lv_cmd(vg_name: &str, vol_name: &str, size_gb: u64) -> String {
        format!(
            "lvcreate -y -L {}G -n {} {} --yes",
            size_gb, vol_name, vg_name
        )
    }

    /// Delete logical volume
    pub fn lvremove_cmd(vg_name: &str, vol_name: &str) -> String {
        format!("lvremove -f /dev/{}/{}", vg_name, vol_name)
    }

    /// Resize/extend logical volume
    pub fn lvextend_cmd(vg_name: &str, vol_name: &str, new_size_gb: u64) -> String {
        format!("lvextend -L {}G /dev/{}/{}", new_size_gb, vg_name, vol_name)
    }

    /// List all volume groups (JSON output)
    pub fn vgs_list_cmd() -> String {
        "vgs --reportformat json --units b --nosuffix -o vg_name,vg_size,vg_free,pv_count,lv_count"
            .to_string()
    }

    /// List all logical volumes (JSON output)
    pub fn lvs_list_cmd() -> String {
        "lvs --reportformat json --units b --nosuffix -o lv_name,vg_name,lv_size,lv_attr,pool_lv,origin,data_percent,metadata_percent,move_pv,mirror_log,copy_percent,convert_lv".to_string()
    }
}

#[allow(dead_code)]
pub async fn run_local_command(cmd_str: &str, description: &str) -> LvmResult<CommandOutput> {
    info!("Executing local command: '{}' ({})", cmd_str, description);

    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd_str)
        .output()
        .await
        .map_err(LvmError::Io)?;

    let cmd_output = CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    };

    if cmd_output.success() {
        debug!(
            "Command '{}' succeeded. Stdout: '{}', Stderr: '{}'",
            cmd_str, cmd_output.stdout, cmd_output.stderr
        );
    } else {
        error!(
            "Command '{}' failed with exit code {}. Stdout: '{}', Stderr: '{}'",
            cmd_str, cmd_output.exit_code, cmd_output.stdout, cmd_output.stderr
        );
    }

    Ok(cmd_output)
}

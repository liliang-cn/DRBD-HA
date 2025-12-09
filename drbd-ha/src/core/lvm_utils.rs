use crate::core::{run_shell_command, CommandOutput, SshCredential, SshManager};
use anyhow::{bail, Result};
use serde::Deserialize;
use std::sync::Arc;

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
    async fn execute(&self, command: &str, description: &str) -> Result<CommandOutput> {
        if let Some((manager, host, port, user, credential)) = &self.ssh {
            manager
                .execute(host, *port, user, credential, command)
                .await
                .map_err(|e| anyhow::anyhow!("Remote command failed on {}: {}", host, e))
        } else {
            run_shell_command(command, description)
                .await
                .map_err(|e| anyhow::anyhow!("Local command failed: {}", e))
        }
    }

    /// Get info about a specific LVM Volume Group using JSON output
    pub async fn get_vg_info(&self, vg_name: &str) -> Result<Option<LvmVgInfo>> {
        let cmd = format!(
            "vgs --reportformat json --units b --nosuffix -o vg_name,vg_size,vg_free,pv_count,lv_count {}",
            vg_name
        );
        let output = self.execute(&cmd, "Get LVM VG info").await?;

        if !output.success() {
            if output.stderr.contains("not found") {
                return Ok(None);
            }
            bail!("Failed to get VG info: {}", output.stderr);
        }

        let vgs = parse_vgs_json(&output.stdout)?;
        Ok(vgs.into_iter().find(|v| v.name == vg_name))
    }

    /// List all LVM Volume Groups using JSON output
    pub async fn list_vg_info(&self) -> Result<Vec<LvmVgInfo>> {
        let cmd = "vgs --reportformat json --units b --nosuffix -o vg_name,vg_size,vg_free,pv_count,lv_count";
        let output = self.execute(cmd, "List all LVM VGs").await?;

        if !output.success() {
            bail!("Failed to list VGs: {}", output.stderr);
        }

        parse_vgs_json(&output.stdout)
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

fn parse_vgs_json(json_str: &str) -> Result<Vec<LvmVgInfo>> {
    let report: VgsReport = serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse vgs JSON: {}", e))?;

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
pub async fn get_vg_info(vg_name: &str) -> Result<Option<LvmVgInfo>> {
    LvmClient::new_local().get_vg_info(vg_name).await
}

/// List all LVM Volume Groups (Local).
/// Wraps `LvmClient::new_local().list_vg_info()`
pub async fn list_vg_info() -> Result<Vec<LvmVgInfo>> {
    LvmClient::new_local().list_vg_info().await
}

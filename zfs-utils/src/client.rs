use crate::error::{ZfsError, ZfsResult};
use serde::Serialize;
use ssh_cmd::{CommandOutput, SshCredential, SshManager};
use std::sync::Arc;

#[cfg(not(test))]
use crate::cmd::run_local_command;

/// Zpool availability check result
#[derive(Debug, Clone, Serialize)]
pub struct ZpoolCheckResult {
    pub installed: bool,
    pub available: bool,
    pub version: Option<String>,
    pub message: String,
    pub pools: Vec<ZpoolStatus>,
}

/// Zpool status information
#[derive(Debug, Clone, Serialize)]
pub struct ZpoolStatus {
    pub name: String,
    pub size: String,
    pub capacity: String,
    pub health: String,
}

/// Represents ZFS Pool information
#[derive(Debug, Clone)]
pub struct ZfsPoolInfo {
    pub name: String,
    pub size: u64,            // Total size in bytes
    pub allocated: u64,       // Allocated size in bytes
    pub free: u64,            // Free size in bytes
    pub capacity: f32,        // Capacity percentage (0.0-100.0)
    pub health: String,       // Health status (ONLINE, DEGRADED, FAULTED, etc.)
    pub devices: Vec<String>, // Underlying devices
}

/// Represents ZFS Dataset information
#[derive(Debug, Clone)]
pub struct ZfsDatasetInfo {
    pub name: String,
    pub type_: String,       // filesystem, volume, snapshot, bookmark
    pub used: u64,           // Used space in bytes
    pub available: u64,      // Available space in bytes
    pub referenced: u64,     // Referenced space in bytes
    pub mountpoint: String,  // Mount point
    pub pool: String,        // Parent pool
    pub origin: String,      // Origin snapshot (for clones)
    pub compression: String, // Compression algorithm
}

/// Represents ZFS Thin Provisioning information (for volumes)
#[derive(Debug, Clone)]
pub struct ZfsThinVolumeInfo {
    pub name: String,
    pub pool: String,
    pub size: u64,              // Volume size in bytes
    pub used: u64,              // Actually used space in bytes
    pub referenced: u64,        // Referenced space in bytes
    pub logical_used: u64,      // Logical space used by volume
    pub compression_ratio: f32, // Compression ratio
    pub sparse: bool,           // Whether volume is sparse
    pub reservation: u64,       // Space reservation
}

/// Client for querying ZFS information (local or remote)
pub struct ZfsClient {
    #[allow(dead_code)] // Field is used in non-test builds, but triggered as unused in tests
    ssh: Option<(Arc<SshManager>, String, u16, String, SshCredential)>,
}

impl ZfsClient {
    /// Create a new local ZFS client
    pub fn new_local() -> Self {
        Self { ssh: None }
    }

    /// Create a new remote ZFS client
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
    async fn execute(&self, command: &str, description: &str) -> ZfsResult<CommandOutput> {
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
                .map_err(ZfsError::Ssh)
        } else {
            run_local_command(command, description).await
        }
    }

    /// Get info about a specific ZFS Pool
    pub async fn get_pool_info(&self, pool_name: &str) -> ZfsResult<Option<ZfsPoolInfo>> {
        let cmd = format!(
            "zpool list -H -o name,size,alloc,free,cap,health {}",
            pool_name
        );
        let output = self.execute(&cmd, "Get ZFS pool info").await?;

        if !output.success() {
            if output.stderr.contains("does not exist") {
                return Ok(None);
            }
            return Err(ZfsError::Execution(output.stderr));
        }

        parse_pool_line(&output.stdout)
    }

    /// List all ZFS Pools
    pub async fn list_pool_info(&self) -> ZfsResult<Vec<ZfsPoolInfo>> {
        let cmd = "zpool list -H -o name,size,alloc,free,cap,health";
        let output = self.execute(cmd, "List all ZFS pools").await?;

        if !output.success() {
            return Err(ZfsError::Execution(output.stderr));
        }

        parse_pool_lines(&output.stdout)
    }

    /// Get devices for a specific pool
    pub async fn get_pool_devices(&self, pool_name: &str) -> ZfsResult<Vec<String>> {
        let cmd = format!("zpool list -H -v {}", pool_name);
        let output = self.execute(&cmd, "Get ZFS pool devices").await?;

        if !output.success() {
            return Err(ZfsError::Execution(output.stderr));
        }

        parse_pool_devices(&output.stdout)
    }

    /// List all ZFS Datasets
    pub async fn list_datasets(&self, pool_name: Option<&str>) -> ZfsResult<Vec<ZfsDatasetInfo>> {
        let base_cmd =
            "zfs list -H -o name,type,used,avail,refer,mountpoint,pool,origin,compression";
        let cmd = if let Some(pool) = pool_name {
            format!("{} -r {}", base_cmd, pool)
        } else {
            base_cmd.to_string()
        };

        let output = self.execute(&cmd, "List ZFS datasets").await?;

        if !output.success() {
            return Err(ZfsError::Execution(output.stderr));
        }

        parse_dataset_lines(&output.stdout)
    }

    /// Create ZFS Pool
    pub async fn create_pool(&self, pool_name: &str, devices: &[String]) -> ZfsResult<()> {
        let device_list = devices.join(" ");
        let cmd = format!("zpool create {} {}", pool_name, device_list);
        let output = self
            .execute(
                &cmd,
                &format!(
                    "Create ZFS pool '{}' with devices: {}",
                    pool_name, device_list
                ),
            )
            .await?;

        if !output.success() {
            return Err(ZfsError::Execution(format!(
                "Failed to create ZFS pool: {}",
                output.stderr
            )));
        }
        Ok(())
    }

    /// Create ZFS Dataset (filesystem)
    pub async fn create_dataset(
        &self,
        pool_name: &str,
        dataset_name: &str,
        properties: Option<&[(String, String)]>,
    ) -> ZfsResult<String> {
        let full_name = format!("{}/{}", pool_name, dataset_name);
        let mut cmd = format!("zfs create {}", full_name);

        if let Some(props) = properties {
            for (key, value) in props {
                cmd.push_str(&format!(" -o {}={}", key, value));
            }
        }

        let output = self
            .execute(&cmd, &format!("Create ZFS dataset '{}'", full_name))
            .await?;

        if !output.success() {
            return Err(ZfsError::Execution(format!(
                "Failed to create ZFS dataset: {}",
                output.stderr
            )));
        }
        Ok(full_name)
    }

    /// Create ZFS Volume (block device)
    pub async fn create_volume(
        &self,
        pool_name: &str,
        volume_name: &str,
        size_gb: u64,
        properties: Option<&[(String, String)]>,
    ) -> ZfsResult<String> {
        let full_name = format!("{}/{}", pool_name, volume_name);
        let mut cmd = format!("zfs create -V {}G {}", size_gb, full_name);

        if let Some(props) = properties {
            for (key, value) in props {
                cmd.push_str(&format!(" -o {}={}", key, value));
            }
        }

        let output = self
            .execute(
                &cmd,
                &format!("Create ZFS volume '{}' of {}GB", full_name, size_gb),
            )
            .await?;

        if !output.success() {
            return Err(ZfsError::Execution(format!(
                "Failed to create ZFS volume: {}",
                output.stderr
            )));
        }
        Ok(format!("/dev/zvol/{}/{}", pool_name, volume_name))
    }

    /// Delete ZFS Dataset or Volume
    pub async fn delete_dataset(&self, dataset_name: &str, recursive: bool) -> ZfsResult<()> {
        let mut cmd = "zfs destroy".to_string();
        if recursive {
            cmd.push_str(" -r");
        }
        cmd.push(' ');
        cmd.push_str(dataset_name);

        let output = self
            .execute(&cmd, &format!("Delete ZFS dataset '{}'", dataset_name))
            .await?;

        if !output.success() {
            return Err(ZfsError::Execution(format!(
                "Failed to delete ZFS dataset: {}",
                output.stderr
            )));
        }
        Ok(())
    }

    /// Resize ZFS Volume
    pub async fn resize_volume(
        &self,
        pool_name: &str,
        volume_name: &str,
        new_size_gb: u64,
    ) -> ZfsResult<()> {
        let full_name = format!("{}/{}", pool_name, volume_name);
        let cmd = format!("zfs set volsize={}G {}", new_size_gb, full_name);
        let output = self
            .execute(
                &cmd,
                &format!("Resize ZFS volume '{}' to {}GB", full_name, new_size_gb),
            )
            .await?;

        if !output.success() {
            return Err(ZfsError::Execution(format!(
                "Failed to resize ZFS volume: {}",
                output.stderr
            )));
        }
        Ok(())
    }

    /// Set ZFS property
    pub async fn set_property(
        &self,
        dataset_name: &str,
        property: &str,
        value: &str,
    ) -> ZfsResult<()> {
        let cmd = format!("zfs set {}={} {}", property, value, dataset_name);
        let output = self
            .execute(
                &cmd,
                &format!(
                    "Set property '{}'='{}' on dataset '{}'",
                    property, value, dataset_name
                ),
            )
            .await?;

        if !output.success() {
            return Err(ZfsError::Execution(format!(
                "Failed to set ZFS property: {}",
                output.stderr
            )));
        }
        Ok(())
    }

    /// Create ZFS Thin Volume with sparse allocation
    pub async fn create_thin_volume(
        &self,
        pool_name: &str,
        volume_name: &str,
        size_gb: u64,
        properties: Option<&[(String, String)]>,
    ) -> ZfsResult<String> {
        let full_name = format!("{}/{}", pool_name, volume_name);
        let mut cmd = format!("zfs create -V {}G -s {}", size_gb, full_name);

        if let Some(props) = properties {
            for (key, value) in props {
                cmd.push_str(&format!(" -o {}={}", key, value));
            }
        }

        let output = self
            .execute(
                &cmd,
                &format!(
                    "Create ZFS thin volume '{}' of {}GB (sparse)",
                    full_name, size_gb
                ),
            )
            .await?;

        if !output.success() {
            return Err(ZfsError::Execution(format!(
                "Failed to create ZFS thin volume: {}",
                output.stderr
            )));
        }
        Ok(format!("/dev/zvol/{}/{}", pool_name, volume_name))
    }

    /// Get detailed information about ZFS volumes (including thin provisioning)
    pub async fn list_thin_volumes(
        &self,
        pool_name: Option<&str>,
    ) -> ZfsResult<Vec<ZfsThinVolumeInfo>> {
        let base_cmd = "zfs list -H -o name,used,volsize,refer,logicalused,compressratio,sparse,volsize,reservation,pool -t volume";
        let cmd = if let Some(pool) = pool_name {
            format!("{} -r {}", base_cmd, pool)
        } else {
            base_cmd.to_string()
        };

        let output = self.execute(&cmd, "List ZFS thin volumes").await?;

        if !output.success() {
            return Err(ZfsError::Execution(output.stderr));
        }

        parse_thin_volume_lines(&output.stdout)
    }

    /// Get thin provisioning information for a specific volume
    pub async fn get_thin_volume_info(
        &self,
        volume_name: &str,
    ) -> ZfsResult<Option<ZfsThinVolumeInfo>> {
        let cmd = format!("zfs list -H -o name,used,volsize,refer,logicalused,compressratio,sparse,volsize,reservation,pool -t volume {}", volume_name);
        let output = self.execute(&cmd, "Get ZFS thin volume info").await?;

        if !output.success() {
            return Err(ZfsError::Execution(output.stderr));
        }

        parse_thin_volume_line(&output.stdout)
    }

    /// Set space reservation for a volume (controls thin provisioning behavior)
    pub async fn set_volume_reservation(
        &self,
        volume_name: &str,
        reservation_gb: u64,
    ) -> ZfsResult<()> {
        let cmd = format!("zfs set reservation={}G {}", reservation_gb, volume_name);
        let output = self
            .execute(
                &cmd,
                &format!(
                    "Set reservation {}G on volume '{}'",
                    reservation_gb, volume_name
                ),
            )
            .await?;

        if !output.success() {
            return Err(ZfsError::Execution(format!(
                "Failed to set volume reservation: {}",
                output.stderr
            )));
        }
        Ok(())
    }

    /// Set refreservation for a volume (refers to referenced space)
    pub async fn set_volume_refreservation(
        &self,
        volume_name: &str,
        refreservation_gb: u64,
    ) -> ZfsResult<()> {
        let cmd = format!(
            "zfs set refreservation={}G {}",
            refreservation_gb, volume_name
        );
        let output = self
            .execute(
                &cmd,
                &format!(
                    "Set refreservation {}G on volume '{}'",
                    refreservation_gb, volume_name
                ),
            )
            .await?;

        if !output.success() {
            return Err(ZfsError::Execution(format!(
                "Failed to set volume refreservation: {}",
                output.stderr
            )));
        }
        Ok(())
    }

    /// Create a clone from a snapshot (inheriting thin provisioning characteristics)
    pub async fn clone_volume(
        &self,
        snapshot_name: &str,
        clone_name: &str,
        properties: Option<&[(String, String)]>,
    ) -> ZfsResult<()> {
        let mut cmd = format!("zfs clone {}", snapshot_name);

        if let Some(props) = properties {
            for (key, value) in props {
                cmd.push_str(&format!(" -o {}={}", key, value));
            }
        }

        cmd.push(' ');
        cmd.push_str(clone_name);

        let output = self
            .execute(
                &cmd,
                &format!(
                    "Clone volume '{}' from snapshot '{}'",
                    clone_name, snapshot_name
                ),
            )
            .await?;

        if !output.success() {
            return Err(ZfsError::Execution(format!(
                "Failed to clone volume: {}",
                output.stderr
            )));
        }
        Ok(())
    }
}

fn parse_pool_line(line: &str) -> ZfsResult<Option<ZfsPoolInfo>> {
    let parts: Vec<&str> = line.trim().split('\t').collect();
    if parts.len() != 6 {
        return Ok(None);
    }

    let name = parts[0].to_string();
    let size = parse_size(parts[1])?;
    let allocated = parse_size(parts[2])?;
    let free = parse_size(parts[3])?;
    let capacity = parts[4].trim_end_matches('%').parse::<f32>().unwrap_or(0.0);
    let health = parts[5].to_string();

    Ok(Some(ZfsPoolInfo {
        name,
        size,
        allocated,
        free,
        capacity,
        health,
        devices: Vec::new(), // Will be filled separately
    }))
}

fn parse_pool_lines(output: &str) -> ZfsResult<Vec<ZfsPoolInfo>> {
    let mut pools = Vec::new();
    for line in output.lines() {
        if let Some(pool) = parse_pool_line(line)? {
            pools.push(pool);
        }
    }
    Ok(pools)
}

fn parse_pool_devices(output: &str) -> ZfsResult<Vec<String>> {
    let mut devices = Vec::new();
    let mut pool_name = String::new();

    for line in output.lines() {
        // Skip header and empty lines
        if line.is_empty() || line.starts_with("NAME") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();

        // Extract pool name from first line
        if pool_name.is_empty() && !parts.is_empty() {
            pool_name = parts[0].to_string();
        }

        // Look for device lines (typically have more than 2 parts and don't start with pool name)
        if parts.len() >= 3 {
            let last_part = parts.last().unwrap();
            if !last_part.starts_with(&pool_name) && last_part.starts_with("/dev/") {
                devices.push(last_part.to_string());
            }
        }
    }
    Ok(devices)
}

fn parse_dataset_lines(output: &str) -> ZfsResult<Vec<ZfsDatasetInfo>> {
    let mut datasets = Vec::new();
    for line in output.lines() {
        if let Some(dataset) = parse_dataset_line(line)? {
            datasets.push(dataset);
        }
    }
    Ok(datasets)
}

fn parse_dataset_line(line: &str) -> ZfsResult<Option<ZfsDatasetInfo>> {
    let parts: Vec<&str> = line.trim().split('\t').collect();
    if parts.len() != 9 {
        return Ok(None);
    }

    let name = parts[0].to_string();
    let type_ = parts[1].to_string();
    let used = parse_size(parts[2])?;
    let available = parse_size(parts[3])?;
    let referenced = parse_size(parts[4])?;
    let mountpoint = parts[5].to_string();
    let pool = parts[6].to_string();
    let origin = parts[7].to_string();
    let compression = parts[8].to_string();

    Ok(Some(ZfsDatasetInfo {
        name,
        type_,
        used,
        available,
        referenced,
        mountpoint,
        pool,
        origin,
        compression,
    }))
}

fn parse_size(size_str: &str) -> ZfsResult<u64> {
    let size_str = size_str.trim();
    if size_str.is_empty() || size_str == "-" {
        return Ok(0);
    }

    // Handle different size units (K, M, G, T, P)
    let (num_str, unit) = if size_str.len() > 1 {
        let (base, suffix) = size_str.split_at(size_str.len() - 1);
        if suffix.chars().next().unwrap().is_ascii_alphabetic() {
            (base, suffix)
        } else {
            (size_str, "")
        }
    } else {
        (size_str, "")
    };

    let base_num: f64 = num_str.parse().unwrap_or(0.0);

    let multiplier = match unit {
        "K" => 1024.0,
        "M" => 1024.0 * 1024.0,
        "G" => 1024.0 * 1024.0 * 1024.0,
        "T" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        "P" => 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };

    Ok((base_num * multiplier) as u64)
}

// Thin volume parsing functions
fn parse_thin_volume_lines(output: &str) -> ZfsResult<Vec<ZfsThinVolumeInfo>> {
    let mut volumes = Vec::new();
    for line in output.lines() {
        if let Some(volume) = parse_thin_volume_line(line)? {
            volumes.push(volume);
        }
    }
    Ok(volumes)
}

fn parse_thin_volume_line(line: &str) -> ZfsResult<Option<ZfsThinVolumeInfo>> {
    let parts: Vec<&str> = line.trim().split('\t').collect();
    if parts.len() < 10 {
        return Ok(None);
    }

    let name = parts[0].to_string();
    let used = parse_size(parts[1])?;
    let size = parse_size(parts[2])?;
    let referenced = parse_size(parts[3])?;
    let logical_used = parse_size(parts[4])?;
    let compression_ratio = parse_compression_ratio(parts[5])?;
    let sparse = parts[6] == "yes";
    let reservation = parse_size(parts[7])?;
    let pool = parts[8].to_string();

    Ok(Some(ZfsThinVolumeInfo {
        name,
        pool,
        size,
        used,
        referenced,
        logical_used,
        compression_ratio,
        sparse,
        reservation,
    }))
}

fn parse_compression_ratio(ratio_str: &str) -> ZfsResult<f32> {
    if ratio_str.is_empty() || ratio_str == "-" {
        return Ok(1.0);
    }

    // ZFS compression ratio is usually like "1.23x" or "-"
    let clean_ratio = ratio_str.trim_end_matches('x');
    clean_ratio
        .parse::<f32>()
        .map_err(|_| ZfsError::Execution(format!("Invalid compression ratio: {}", ratio_str)))
}

/// Get ZFS Pool information by name (Local).
/// Wraps `ZfsClient::new_local().get_pool_info(pool_name)`
pub async fn get_pool_info(pool_name: &str) -> ZfsResult<Option<ZfsPoolInfo>> {
    ZfsClient::new_local().get_pool_info(pool_name).await
}

/// List all ZFS Pools (Local).
/// Wraps `ZfsClient::new_local().list_pool_info()`
pub async fn list_pool_info() -> ZfsResult<Vec<ZfsPoolInfo>> {
    ZfsClient::new_local().list_pool_info().await
}

/// List all ZFS Datasets (Local).
/// Wraps `ZfsClient::new_local().list_datasets()`
pub async fn list_datasets() -> ZfsResult<Vec<ZfsDatasetInfo>> {
    ZfsClient::new_local().list_datasets(None).await
}

/// List all ZFS Thin Volumes (Local).
/// Wraps `ZfsClient::new_local().list_thin_volumes()`
pub async fn list_thin_volumes() -> ZfsResult<Vec<ZfsThinVolumeInfo>> {
    ZfsClient::new_local().list_thin_volumes(None).await
}

/// Get ZFS Thin Volume information by name (Local).
/// Wraps `ZfsClient::new_local().get_thin_volume_info(volume_name)`
pub async fn get_thin_volume_info(volume_name: &str) -> ZfsResult<Option<ZfsThinVolumeInfo>> {
    ZfsClient::new_local()
        .get_thin_volume_info(volume_name)
        .await
}

impl ZfsClient {
    /// Check if zpool is installed and available
    pub async fn check_zpool(&self) -> ZfsResult<ZpoolCheckResult> {
        // First check if zpool command exists
        let which_output = self
            .execute("which zpool", "Check if zpool is installed")
            .await?;

        let installed = which_output.exit_code == 0;

        if !installed {
            return Ok(ZpoolCheckResult {
                installed: false,
                available: false,
                version: None,
                message: "zpool command not found. ZFS is not installed on this system."
                    .to_string(),
                pools: vec![],
            });
        }

        // Check zpool version
        let version_output = self
            .execute("zpool version 2>&1 | head -n 1", "Get zpool version")
            .await?;
        let version = if version_output.exit_code == 0 && !version_output.stdout.trim().is_empty() {
            Some(version_output.stdout.trim().to_string())
        } else {
            None
        };

        // Try to list pools to check if zpool is functional
        let list_output = self
            .execute(
                "zpool list -H -o name,size,capacity,health 2>/dev/null",
                "List zpools",
            )
            .await?;

        let available = list_output.exit_code == 0;
        let mut pools = vec![];

        if available {
            // Parse zpool list output
            // Format: name\tsize\tcapacity\thealth
            for line in list_output.stdout.lines() {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 4 {
                    pools.push(ZpoolStatus {
                        name: parts[0].to_string(),
                        size: parts[1].to_string(),
                        capacity: parts[2].to_string(),
                        health: parts[3].to_string(),
                    });
                }
            }
        }

        let message = if available {
            format!(
                "ZFS is installed and available. Found {} pool(s).",
                pools.len()
            )
        } else {
            "ZFS is installed but zpool command failed. ZFS kernel module may not be loaded."
                .to_string()
        };

        Ok(ZpoolCheckResult {
            installed: true,
            available,
            version,
            message,
            pools,
        })
    }
}

/// Check if zpool is installed and available (Local).
/// Wraps `ZfsClient::new_local().check_zpool()`
pub async fn check_zpool() -> ZfsResult<ZpoolCheckResult> {
    ZfsClient::new_local().check_zpool().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::tests::{clear_mocks, get_recorded_commands_list, push_mock_output};
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_get_pool_info() {
        clear_mocks();

        let output_str = "testpool\t10.0G\t2.0G\t8.0G\t20%\tONLINE\n";
        push_mock_output(CommandOutput {
            stdout: output_str.to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        });

        let client = ZfsClient::new_local();
        let pool_info = client.get_pool_info("testpool").await.unwrap();

        assert!(pool_info.is_some());
        let info = pool_info.unwrap();
        assert_eq!(info.name, "testpool");
        assert_eq!(info.size, 10 * 1024 * 1024 * 1024); // 10GB
        assert_eq!(info.allocated, 2 * 1024 * 1024 * 1024); // 2GB
        assert_eq!(info.free, 8 * 1024 * 1024 * 1024); // 8GB
        assert_eq!(info.capacity, 20.0);
        assert_eq!(info.health, "ONLINE");

        let cmds = get_recorded_commands_list();
        assert_eq!(cmds.len(), 1);
        assert!(cmds[0].contains("zpool list"));
        assert!(cmds[0].contains("testpool"));
    }

    #[tokio::test]
    #[serial]
    async fn test_get_pool_info_not_found() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "".to_string(),
            stderr: "cannot open 'nonexistent': pool does not exist".to_string(),
            exit_code: 1,
        });

        let client = ZfsClient::new_local();
        let pool_info = client.get_pool_info("nonexistent").await.unwrap();

        assert!(pool_info.is_none());
    }

    #[tokio::test]
    #[serial]
    async fn test_create_dataset() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        });

        let client = ZfsClient::new_local();
        let result = client
            .create_dataset(
                "testpool",
                "testfs",
                Some(&[("compression".to_string(), "lz4".to_string())]),
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "testpool/testfs");

        let cmds = get_recorded_commands_list();
        assert_eq!(cmds.len(), 1);
        assert!(cmds[0].contains("zfs create"));
        assert!(cmds[0].contains("-o compression=lz4"));
        assert!(cmds[0].contains("testpool/testfs"));
    }

    #[tokio::test]
    #[serial]
    async fn test_create_volume() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        });

        let client = ZfsClient::new_local();
        let result = client.create_volume("testpool", "testvol", 10, None).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "/dev/zvol/testpool/testvol");

        let cmds = get_recorded_commands_list();
        assert_eq!(cmds.len(), 1);
        assert!(cmds[0].contains("zfs create -V 10G testpool/testvol"));
    }

    #[tokio::test]
    #[serial]
    async fn test_parse_size() {
        assert_eq!(parse_size("100").unwrap(), 100);
        assert_eq!(parse_size("1K").unwrap(), 1024);
        assert_eq!(parse_size("1M").unwrap(), 1024 * 1024);
        assert_eq!(parse_size("1G").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(
            parse_size("2.5G").unwrap(),
            (2.5 * 1024.0 * 1024.0 * 1024.0) as u64
        );
        assert_eq!(parse_size("-").unwrap(), 0);
        assert_eq!(parse_size("").unwrap(), 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_thin_volume() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        });

        let client = ZfsClient::new_local();
        let result = client
            .create_thin_volume("testpool", "thinvol", 50, None)
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "/dev/zvol/testpool/thinvol");

        let cmds = get_recorded_commands_list();
        assert_eq!(cmds.len(), 1);
        assert!(cmds[0].contains("zfs create -V 50G -s testpool/thinvol"));
    }

    #[tokio::test]
    #[serial]
    async fn test_set_volume_reservation() {
        clear_mocks();

        push_mock_output(CommandOutput {
            stdout: "".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        });

        let client = ZfsClient::new_local();
        let result = client.set_volume_reservation("testpool/thinvol", 10).await;

        assert!(result.is_ok());

        let cmds = get_recorded_commands_list();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "zfs set reservation=10G testpool/thinvol");
    }

    #[tokio::test]
    #[serial]
    async fn test_parse_compression_ratio() {
        assert_eq!(parse_compression_ratio("1.23x").unwrap(), 1.23);
        assert_eq!(parse_compression_ratio("2.00x").unwrap(), 2.00);
        assert_eq!(parse_compression_ratio("-").unwrap(), 1.0);
        assert_eq!(parse_compression_ratio("").unwrap(), 1.0);
        assert!(parse_compression_ratio("invalid").is_err());
    }
}

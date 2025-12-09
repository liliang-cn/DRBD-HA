//! NFS HA Generator
//!
//! Handles configuration for Highly Available NFS services.
//! Follows the architecture: LVM -> DRBD -> XFS -> Reactor (VIP + exportfs).
//! Crucially, manages NFS state data (/var/lib/nfs) to prevent stale file handles.

use crate::models::NfsConfig;
use crate::core::run_shell_command;
use crate::error::AppResult;
use std::path::Path;
use tracing::{info, warn};

pub struct NfsGenerator;

impl NfsGenerator {
    /// Generate the OCF resource string for `drbd-reactor`
    /// This dynamically manages exports, avoiding manual edits to `/etc/exports`
    pub fn generate_ocf_exportfs(
        resource_name: &str,
        mount_point: &str,
        config: &NfsConfig,
        fsid: u32,
    ) -> String {
        // Format: ocf:heartbeat:exportfs name=nfs_exp fsid=1 directory=/exports/share clientspec='192.168.1.0/24' options='rw,sync,no_root_squash'
        // Note: We take the first allowed network. Complex multi-network setups might need multiple OCF resources.
        let clientspec = config.allowed_networks.first().map(|s| s.as_str()).unwrap_or("*");
        
        format!(
            "ocf:heartbeat:exportfs name={}_exp fsid={} directory={} clientspec='{}' options='{}'",
            resource_name,
            fsid,
            mount_point,
            clientspec,
            config.options
        )
    }

    /// Setup NFS state directory for High Availability
    /// This moves /var/lib/nfs to the shared storage to preserve locks and mount states across failovers.
    /// 
    /// Actions:
    /// 1. Create `.nfs_state` directory in the mount point (DRBD storage).
    /// 2. Copy existing /var/lib/nfs content to it (if empty).
    /// 3. Symlink /var/lib/nfs -> {mount_point}/.nfs_state
    pub async fn setup_nfs_state(mount_point: &str) -> AppResult<()> {
        let state_dir = format!("{}/.nfs_state", mount_point);
        let system_nfs_dir = "/var/lib/nfs";
        let backup_nfs_dir = "/var/lib/nfs.bak";

        info!("Setting up NFS state directory at {}", state_dir);

        // 1. Create state directory on shared storage
        if !Path::new(&state_dir).exists() {
            run_shell_command(&format!("mkdir -p {}", state_dir), "Create NFS state dir").await?;
            // Set ownership to nobody:nogroup or similar? standard NFS uses specific uids.
            // For now, let's keep root but ensure permissions.
            run_shell_command(&format!("chmod 755 {}", state_dir), "Chmod NFS state dir").await?;
        }

        // 2. Check if /var/lib/nfs is already a symlink to THIS state dir
        let is_correct_symlink = match std::fs::read_link(system_nfs_dir) {
            Ok(target) => target.to_string_lossy() == state_dir,
            Err(_) => false,
        };

        if is_correct_symlink {
            info!("NFS state directory is already correctly linked");
            return Ok(());
        }

        // 3. Prepare system directory
        // Stop NFS server first to release locks on /var/lib/nfs
        run_shell_command("systemctl stop nfs-server", "Stop NFS server").await?;

        // Backup existing /var/lib/nfs if it's a real directory (not a symlink)
        if Path::new(system_nfs_dir).is_dir() && !Path::new(system_nfs_dir).is_symlink() {
            // Check if we should copy data to shared storage (only if shared is empty)
            // Simplification: Just copy everything if shared is empty
            let is_shared_empty = run_shell_command(&format!("ls -A {}", state_dir), "Check shared empty").await?
                .stdout.trim().is_empty();
            
            if is_shared_empty {
                info!("Copying existing NFS state to shared storage");
                run_shell_command(&format!("cp -a {}/. {}", system_nfs_dir, state_dir), "Copy NFS state").await?;
            }

            info!("Backing up local NFS state directory");
            run_shell_command(&format!("mv {} {}", system_nfs_dir, backup_nfs_dir), "Backup NFS state").await?;
        } else if Path::new(system_nfs_dir).is_symlink() {
            // It's a symlink but pointing somewhere else? Remove it.
            warn!("Removing incorrect symlink at {}", system_nfs_dir);
            run_shell_command(&format!("rm {}", system_nfs_dir), "Remove incorrect link").await?;
        }

        // 4. Create Symlink
        info!("Linking {} -> {}", system_nfs_dir, state_dir);
        run_shell_command(&format!("ln -s {} {}", state_dir, system_nfs_dir), "Link NFS state").await?;

        Ok(())
    }

    /// Calculate a deterministic unique FSID based on resource name
    /// Uses a simple hash to map string to u32
    pub fn generate_fsid(resource_name: &str) -> u32 {
        let mut hash: u32 = 0;
        for b in resource_name.bytes() {
            hash = hash.wrapping_add(b as u32);
            hash = hash.wrapping_add(hash << 10);
            hash ^= hash >> 6;
        }
        hash = hash.wrapping_add(hash << 3);
        hash ^= hash >> 11;
        hash = hash.wrapping_add(hash << 15);
        
        // Ensure > 0
        if hash == 0 { 1 } else { hash }
    }
}

/// Legacy method stubs to satisfy any remaining calls during refactor,
/// although create_profile should now be using the new methods.
impl NfsGenerator {
    pub fn generate_export_config(_name: &str, _mount: &str, _config: &NfsConfig) -> String {
        String::new()
    }
    pub fn get_export_path(_name: &str) -> String {
        String::new()
    }
}



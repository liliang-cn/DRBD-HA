use crate::error::{LvmError, LvmResult};
use regex::Regex;
use tokio::fs;
use tracing::{info, warn};

const LVM_CONF_PATH: &str = "/etc/lvm/lvm.conf";
const LVM_DRBD_FILTER: &str =
    r###"    filter = [ \"a|/dev/sd.*|\", \"a|/dev/nvme.*|\", \"r|/dev/drbd.*|\", \"r|.*|\" ]"###; // Using raw string for regex

/// Configures the LVM filter in /etc/lvm/lvm.conf to prevent scanning DRBD devices.
/// This is crucial to avoid "Duplicate PV" errors in DRBD + LVM setups.
pub async fn configure_lvm_filter() -> LvmResult<()> {
    info!("Checking and configuring LVM filter in {}", LVM_CONF_PATH);

    let content = fs::read_to_string(LVM_CONF_PATH)
        .await
        .map_err(LvmError::Io)?;

    let filter_regex = Regex::new(r"(?m)^\s*filter\s*=\s*\[.*?\]")
        .map_err(|e| LvmError::Config(format!("Invalid regex: {}", e)))?;
    let global_filter_regex = Regex::new(r"(?m)^\s*global_filter\s*=\s*\[.*?\]")
        .map_err(|e| LvmError::Config(format!("Invalid regex: {}", e)))?;

    let mut modified_content = content.clone();
    let drbd_filter_line = LVM_DRBD_FILTER;

    let mut filter_found = false;

    // Check and replace existing 'filter'
    if filter_regex.is_match(&modified_content) {
        modified_content = filter_regex
            .replace_all(&modified_content, drbd_filter_line)
            .to_string();
        filter_found = true;
        info!("Updated existing 'filter' setting in {}", LVM_CONF_PATH);
    }

    // Check and replace existing 'global_filter' (less common, but good to check)
    if global_filter_regex.is_match(&modified_content) {
        // If both filter and global_filter exist, prioritize 'filter' as per LVM's behavior
        // If only global_filter exists, update it.
        // For simplicity, we'll replace global_filter only if 'filter' wasn't found/modified.
        if !filter_found {
            modified_content = global_filter_regex
                .replace_all(&modified_content, drbd_filter_line)
                .to_string();
            filter_found = true;
            info!(
                "Updated existing 'global_filter' setting in {}",
                LVM_CONF_PATH
            );
        } else {
            warn!("Both 'filter' and 'global_filter' found. Prioritizing 'filter'. 'global_filter' might still need manual adjustment if not desired.");
        }
    }

    // If no filter was found, add it to the 'devices' section
    if !filter_found {
        let devices_start_regex = Regex::new(r"(?m)^devices\s*\{")
            .map_err(|e| LvmError::Config(format!("Invalid regex: {}", e)))?;

        if let Some(captures) = devices_start_regex.captures(&modified_content) {
            if let Some(m) = captures.get(0) {
                let insert_pos = m.end();
                modified_content.insert_str(insert_pos, &format!("\n{}", drbd_filter_line));
                info!(
                    "Added 'filter' setting to 'devices' section in {}",
                    LVM_CONF_PATH
                );
            }
        } else {
            return Err(LvmError::Config(format!(
                "Could not find 'devices {{' section in {}. Please configure LVM filter manually.",
                LVM_CONF_PATH
            )));
        }
    }

    // Write back only if content actually changed
    if modified_content != content {
        fs::write(LVM_CONF_PATH, modified_content.as_bytes())
            .await
            .map_err(LvmError::Io)?;
        info!("Successfully configured LVM filter in {}", LVM_CONF_PATH);
    } else {
        info!(
            "LVM filter already correctly configured in {}",
            LVM_CONF_PATH
        );
    }

    Ok(())
}

use crate::error::{DrbdError, DrbdResult};
use regex::Regex;
use std::sync::OnceLock;

static RESOURCE_NAME_REGEX: OnceLock<Regex> = OnceLock::new();
static BLOCK_DEVICE_REGEX: OnceLock<Regex> = OnceLock::new();

pub fn validate_resource_name(name: &str) -> DrbdResult<()> {
    if name.is_empty() {
        return Err(DrbdError::Validation("Resource name cannot be empty".to_string()));
    }
    
    let re = RESOURCE_NAME_REGEX.get_or_init(|| Regex::new(r"^[a-zA-Z0-9_\-\.]+$").unwrap());
    
    if !re.is_match(name) {
        return Err(DrbdError::Validation(format!(
            "Invalid resource name '{}': must contain only alphanumeric characters, underscores, hyphens, and dots",
            name
        )));
    }
    Ok(())
}

pub fn validate_block_device(path: &str) -> DrbdResult<()> {
    if path.is_empty() {
        return Err(DrbdError::Validation("Block device path cannot be empty".to_string()));
    }

    let re = BLOCK_DEVICE_REGEX.get_or_init(|| Regex::new(r"^/dev/[a-zA-Z0-9_/\-\.]+$").unwrap());

    if !re.is_match(path) {
        return Err(DrbdError::Validation(format!(
            "Invalid block device path '{}': must start with /dev/",
            path
        )));
    }
    Ok(())
}

pub fn validate_mount_point(path: &str) -> DrbdResult<()> {
    if path.is_empty() {
        return Err(DrbdError::Validation("Mount point cannot be empty".to_string()));
    }
    if !path.starts_with('/') {
        return Err(DrbdError::Validation(format!(
            "Invalid mount point '{}': must be absolute path",
            path
        )));
    }
    Ok(())
}

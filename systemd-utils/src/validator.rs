use crate::error::{SystemdError, SystemdResult};
use regex::Regex;
use std::sync::OnceLock;

static SERVICE_NAME_REGEX: OnceLock<Regex> = OnceLock::new();

pub fn validate_service_name(name: &str) -> SystemdResult<()> {
    let re = SERVICE_NAME_REGEX.get_or_init(|| {
        // Support all systemd unit types: service, target, socket, device, mount, automount,
        // swap, timer, path, slice, scope
        // Note: systemd unit names can contain letters, digits, :, -, _, ., and @
        // They can also contain * for device units (for glob patterns)
        Regex::new(r"^[a-zA-Z0-9_:.@*\-]+\.(service|target|socket|device|mount|automount|swap|timer|path|slice|scope)$").unwrap()
    });

    if !re.is_match(name) {
        return Err(SystemdError::Validation(format!(
            "Invalid systemd unit name: '{}'. Must end with a valid unit type (.service, .target, .socket, etc.)",
            name
        )));
    }
    Ok(())
}

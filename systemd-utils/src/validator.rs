use crate::error::{SystemdError, SystemdResult};
use regex::Regex;
use std::sync::OnceLock;

static SERVICE_NAME_REGEX: OnceLock<Regex> = OnceLock::new();

pub fn validate_service_name(name: &str) -> SystemdResult<()> {
    let re = SERVICE_NAME_REGEX.get_or_init(|| Regex::new(r"^[a-zA-Z0-9_@.-]+\.service$").unwrap());

    if !re.is_match(name) {
        return Err(SystemdError::Validation(format!(
            "Invalid service name: {}",
            name
        )));
    }
    Ok(())
}

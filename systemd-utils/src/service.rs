use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Service status information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ServiceStatus {
    pub name: String,
    pub active_state: String,
    pub sub_state: String,
    pub load_state: String,
    pub description: String,
}

impl ServiceStatus {
    /// Check if service is running
    pub fn is_running(&self) -> bool {
        self.active_state == "active" && self.sub_state == "running"
    }

    /// Check if service is enabled
    pub fn is_enabled(&self) -> bool {
        self.load_state == "loaded"
    }
}

/// Information about a running service
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ServiceInfo {
    pub name: String,
    pub description: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
}

/// Information about a service unit file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ServiceFileInfo {
    pub name: String,
    pub path: String,
    pub enabled_state: String,
}

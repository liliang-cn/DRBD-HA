use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactorProfileStatus {
    pub name: String,
    pub active_node: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactorServiceDetail {
    pub name: String,
    pub active_node: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactorServiceStatus {
    pub name: String,
    pub active: bool,
    pub state: String,
}

/// Options for drbd-reactorctl status command
#[derive(Debug, Clone, Default)]
pub struct StatusOptions {
    /// Filter to specific DRBD resources
    pub resources: Option<Vec<String>>,
    /// Verbose output
    pub verbose: bool,
}

/// Options for drbd-reactorctl evict command
#[derive(Debug, Clone, Default)]
pub struct EvictOptions {
    /// Positive number of seconds to wait for peer takeover [default: 20]
    pub delay: Option<u32>,
    /// Override checks (multiple plugins per snippet/multiple resources per promoter)
    pub force: bool,
    /// If set the target unit will stay masked
    pub keep_masked: bool,
    /// Unmask targets (clear previous --keep-masked operations)
    pub unmask: bool,
    /// Cluster context
    pub context: Option<String>,
    /// Only use given nodes from the context
    pub nodes: Option<Vec<String>>,
}

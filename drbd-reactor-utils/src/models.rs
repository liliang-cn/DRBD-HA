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

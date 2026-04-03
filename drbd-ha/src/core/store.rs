use crate::error::AppResult;
use crate::models::Node;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

fn default_nodes_file() -> String {
    if let Ok(path) = std::env::var("DRBD_HA_NODES_FILE") {
        return path;
    }

    let legacy_path = "/etc/drbd-ha/nodes.toml";
    if Path::new(legacy_path).exists() {
        return legacy_path.to_string();
    }

    let base_dir = if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var("USERPROFILE")
                    .ok()
                    .map(std::path::PathBuf::from)
            })
            .map(|path| path.join("drbd-ha"))
    } else if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .map(|path| path.join("Library/Application Support/drbd-ha"))
    } else {
        std::env::var("HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .map(|path| path.join(".config/drbd-ha"))
    };

    base_dir
        .unwrap_or_else(|| std::path::PathBuf::from("/etc/drbd-ha"))
        .join("nodes.toml")
        .to_string_lossy()
        .to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodesData {
    nodes: Vec<Node>,
}

#[derive(Clone)]
pub struct NodeStore {
    path: String,
}

impl NodeStore {
    pub fn new(path: Option<String>) -> Self {
        Self {
            path: path.unwrap_or_else(default_nodes_file),
        }
    }

    pub fn get_all(&self) -> AppResult<Vec<Node>> {
        if !Path::new(&self.path).exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&self.path)?;
        let data: NodesData = toml::from_str(&content)
            .map_err(|e| crate::error::AppError::Validation(format!("Invalid TOML: {}", e)))?;
        Ok(data.nodes)
    }

    pub fn get(&self, id: &str) -> AppResult<Option<Node>> {
        let nodes = self.get_all()?;
        Ok(nodes.into_iter().find(|n| n.id == id))
    }

    pub fn insert(&self, node: &Node) -> AppResult<()> {
        let mut nodes = self.get_all()?;
        if let Some(idx) = nodes.iter().position(|n| n.id == node.id) {
            nodes[idx] = node.clone();
        } else {
            nodes.push(node.clone());
        }
        self.save(&nodes)
    }

    pub fn replace_all(&self, nodes: &[Node]) -> AppResult<()> {
        self.save(nodes)
    }

    /// Update an existing node (must exist)
    pub fn update(&self, node: &Node) -> AppResult<()> {
        let mut nodes = self.get_all()?;
        let idx = nodes.iter().position(|n| n.id == node.id).ok_or_else(|| {
            crate::error::AppError::NotFound(format!("Node {} not found", node.id))
        })?;
        nodes[idx] = node.clone();
        self.save(&nodes)
    }

    pub fn delete(&self, id: &str) -> AppResult<bool> {
        let mut nodes = self.get_all()?;
        let len_before = nodes.len();
        nodes.retain(|n| n.id != id);
        if nodes.len() != len_before {
            self.save(&nodes)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn save(&self, nodes: &[Node]) -> AppResult<()> {
        if let Some(parent) = Path::new(&self.path).parent() {
            fs::create_dir_all(parent)?;
        }
        let data = NodesData {
            nodes: nodes.to_vec(),
        };
        let content = toml::to_string_pretty(&data).map_err(|e| {
            crate::error::AppError::Internal(format!("TOML serialization error: {}", e))
        })?;
        fs::write(&self.path, content)?;
        Ok(())
    }
}

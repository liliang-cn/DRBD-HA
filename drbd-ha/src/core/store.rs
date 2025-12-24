use crate::error::AppResult;
use crate::models::Node;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const NODES_FILE: &str = "/etc/drbd-ha/nodes.toml";

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
            path: path.unwrap_or_else(|| NODES_FILE.to_string()),
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
        let content = toml::to_string_pretty(&data)
            .map_err(|e| crate::error::AppError::Internal(format!("TOML serialization error: {}", e)))?;
        fs::write(&self.path, content)?;
        Ok(())
    }
}

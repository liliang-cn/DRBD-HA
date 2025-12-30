use axum::{
    extract::Path,
    Json,
};
use ra_params::{get_agent_metadata, list_agents};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use utoipa::ToSchema;

use crate::error::{AppError, AppResult, ErrorResponse};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AgentSummary {
    pub provider: String,
    pub name: String,
}

// Define DTOs with ToSchema to satisfy utoipa requirements
// These mirror ra_params::models structs but with ToSchema derive

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ResourceAgentDto {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@version")]
    pub version_attr: Option<String>,
    #[serde(rename = "version")]
    pub version_elem: Option<String>,
    #[serde(default)]
    pub longdesc: LocalizedTextDto,
    #[serde(default)]
    pub shortdesc: LocalizedTextDto,
    #[serde(default)]
    pub parameters: ParametersDto,
    #[serde(default)]
    pub actions: ActionsDto,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, ToSchema)]
pub struct LocalizedTextDto {
    #[serde(rename = "@lang", default)]
    pub lang: String,
    #[serde(rename = "$value", default)]
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, ToSchema)]
pub struct ParametersDto {
    #[serde(rename = "parameter", default)]
    pub parameters: Vec<ParameterDto>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ParameterDto {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@unique", default)]
    pub unique: String,
    #[serde(rename = "@required", default)]
    pub required: String,
    #[serde(default)]
    pub longdesc: LocalizedTextDto,
    #[serde(default)]
    pub shortdesc: LocalizedTextDto,
    #[serde(default)]
    pub content: ContentDto,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, ToSchema)]
pub struct ContentDto {
    #[serde(rename = "@type", default)]
    pub type_: String,
    #[serde(rename = "@default", default)]
    pub default: String,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, ToSchema)]
pub struct ActionsDto {
    #[serde(rename = "action", default)]
    pub actions: Vec<ActionDto>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ActionDto {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@timeout", default)]
    pub timeout: String,
    #[serde(rename = "@interval", default)]
    pub interval: String,
    #[serde(rename = "@depth", default)]
    pub depth: String,
}

impl From<ra_params::models::ResourceAgent> for ResourceAgentDto {
    fn from(ra: ra_params::models::ResourceAgent) -> Self {
        // Use serde round-trip for simplicity since structs are identical in JSON structure
        let json = serde_json::to_value(ra).expect("Failed to serialize ResourceAgent");
        serde_json::from_value(json).expect("Failed to deserialize ResourceAgentDto")
    }
}

/// List available OCF resource agents
#[utoipa::path(
    get,
    path = "/api/v1/ha/resource-agents",
    tag = "HA",
    responses(
        (status = 200, description = "List of available resource agents", body = Vec<AgentSummary>),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn list_resource_agents() -> AppResult<Json<Vec<AgentSummary>>> {
    // Determine OCF_ROOT, default to /usr/lib/ocf
    let ocf_root = std::env::var("OCF_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/usr/lib/ocf"));

    let agents = list_agents(&ocf_root).map_err(|e| AppError::Internal(e.to_string()))?;

    let summary = agents
        .into_iter()
        .map(|(p, n)| AgentSummary {
            provider: p,
            name: n,
        })
        .collect();

    Ok(Json(summary))
}

/// Get metadata for a specific resource agent
#[utoipa::path(
    get,
    path = "/api/v1/ha/resource-agents/{provider}/{agent}",
    tag = "HA",
    params(
        ("provider" = String, Path, description = "Agent provider (e.g., heartbeat)"),
        ("agent" = String, Path, description = "Agent name (e.g., IPaddr2)")
    ),
    responses(
        (status = 200, description = "Agent metadata", body = ResourceAgentDto),
        (status = 404, description = "Agent not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn get_resource_agent_metadata(
    Path((provider, agent)): Path<(String, String)>,
) -> AppResult<Json<ResourceAgentDto>> {
    let ocf_root = std::env::var("OCF_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/usr/lib/ocf"));

    let agent_path = ocf_root.join("resource.d").join(&provider).join(&agent);

    if !agent_path.exists() {
        return Err(AppError::NotFound(format!(
            "Agent {}/{} not found",
            provider, agent
        )));
    }

        let (meta, _) = get_agent_metadata(&agent_path).map_err(|e| AppError::Internal(e.to_string()))?;

    

        Ok(Json(meta.into()))

    }

    
// Re-export flattened types from toml_parse module
pub use crate::api::ha::toml_parse::{ResourceAgent, Parameter, Action};

/// All resource agents grouped by provider
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ResourceAgentsByProvider {
    /// Map of provider name to list of agents with metadata
    /// e.g., { "heartbeat": [agent1, agent2], "linbit": [agent3] }
    pub providers: HashMap<String, Vec<ResourceAgent>>,
}

/// Get all resource agents with full metadata, grouped by provider
#[utoipa::path(
    get,
    path = "/api/v1/ha/resource-agents/all",
    tag = "HA",
    responses(
        (status = 200, description = "All resource agents grouped by provider", body = ResourceAgentsByProvider),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn list_all_resource_agents() -> AppResult<Json<ResourceAgentsByProvider>> {
    let ocf_root = std::env::var("OCF_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/usr/lib/ocf"));

    let resource_d = ocf_root.join("resource.d");

    // Check if resource.d exists
    if !resource_d.exists() {
        return Ok(Json(ResourceAgentsByProvider {
            providers: HashMap::new(),
        }));
    }

    let mut providers_map: HashMap<String, Vec<ResourceAgent>> = HashMap::new();

    // Read all provider directories
    let providers = fs::read_dir(&resource_d).map_err(|e| {
        AppError::Internal(format!("Failed to read resource.d directory: {}", e))
    })?;

    for provider_entry in providers {
        let provider_entry = provider_entry.map_err(|e| {
            AppError::Internal(format!("Failed to read provider entry: {}", e))
        })?;

        let provider_path = provider_entry.path();

        // Skip if not a directory
        if !provider_path.is_dir() {
            continue;
        }

        let provider_name = match provider_path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => continue,
        };

        // Skip if provider name starts with dot
        if provider_name.starts_with('.') {
            continue;
        }

        // Read all agents in this provider directory
        let agents = match fs::read_dir(&provider_path) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        let mut provider_agents: Vec<ResourceAgent> = Vec::new();

        for agent_entry in agents {
            let agent_entry = match agent_entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            let agent_path = agent_entry.path();

            // Skip if not a file (or symlink to file)
            if !agent_path.is_file() {
                continue;
            }

            // Get agent name from file name
            let agent_name = match agent_path.file_name() {
                Some(name) => name.to_string_lossy().to_string(),
                None => continue,
            };

            // Get metadata for this agent
            let (ra_metadata, _) = match get_agent_metadata(&agent_path) {
                Ok(meta) => meta,
                Err(e) => {
                    tracing::warn!(
                        "Failed to get metadata for {}/{}: {}",
                        provider_name,
                        agent_name,
                        e
                    );
                    continue;
                }
            };

            // Convert to flattened format (using the From impl in toml_parse)
            let agent: ResourceAgent = ResourceAgent::from(&ra_metadata);
            provider_agents.push(agent);
        }

        // Sort agents by name
        provider_agents.sort_by(|a, b| a.name.cmp(&b.name));

        // Only add non-empty providers
        if !provider_agents.is_empty() {
            providers_map.insert(provider_name, provider_agents);
        }
    }

    Ok(Json(ResourceAgentsByProvider {
        providers: providers_map,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_agents_by_provider_serialization() {
        let mut providers = HashMap::new();
        
        // Add a sample agent
        providers.insert("heartbeat".to_string(), vec![
            ResourceAgent {
                name: "IPaddr2".to_string(),
                version: "1.0".to_string(),
                shortdesc: "Manages virtual IPv4 addresses".to_string(),
                longdesc: "Long description".to_string(),
                parameters: vec![],
                actions: vec![],
            }
        ]);
        
        let response = ResourceAgentsByProvider {
            providers,
        };
        
        // Test serialization
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("heartbeat"));
        assert!(json.contains("IPaddr2"));
    }
}

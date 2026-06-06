use axum::{extract::Path, Json};
use ra_params::{get_agent_metadata, list_agents};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

    let agents = list_agents(&ocf_root).await.map_err(|e| AppError::Internal(e.to_string()))?;

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

    let (meta, _) = get_agent_metadata(&agent_path).await.map_err(|e| {
        AppError::NotFound(format!(
            "Agent {}/{} not found or metadata fetch failed: {}",
            provider, agent, e
        ))
    })?;

    Ok(Json(meta.into()))
}

// Re-export flattened types from toml_parse module
pub use crate::api::ha::toml_parse::{Action, Parameter, ResourceAgent};

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

    let mut providers_map: HashMap<String, Vec<ResourceAgent>> = HashMap::new();

    let discovered_agents =
        list_agents(&ocf_root).await.map_err(|e| AppError::Internal(e.to_string()))?;

    for (provider_name, agent_name) in discovered_agents {
        let agent_path = ocf_root
            .join("resource.d")
            .join(&provider_name)
            .join(&agent_name);
        let (ra_metadata, _) = match get_agent_metadata(&agent_path).await {
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

        let agent: ResourceAgent = ResourceAgent::from(&ra_metadata);
        providers_map.entry(provider_name).or_default().push(agent);
    }

    for provider_agents in providers_map.values_mut() {
        provider_agents.sort_by(|a, b| a.name.cmp(&b.name));
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
        providers.insert(
            "heartbeat".to_string(),
            vec![ResourceAgent {
                name: "IPaddr2".to_string(),
                version: "1.0".to_string(),
                shortdesc: "Manages virtual IPv4 addresses".to_string(),
                longdesc: "Long description".to_string(),
                parameters: vec![],
                actions: vec![],
            }],
        );

        let response = ResourceAgentsByProvider { providers };

        // Test serialization
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("heartbeat"));
        assert!(json.contains("IPaddr2"));
    }
}

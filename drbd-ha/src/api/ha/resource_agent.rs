use axum::{
    extract::Path,
    Json,
};
use ra_params::{get_agent_metadata, list_agents};
use serde::{Deserialize, Serialize};
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

    
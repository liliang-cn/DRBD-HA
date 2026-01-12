//! API for parsing and editing drbd-reactor TOML configuration files
//! with structured OCF agent information

use axum::{
    extract::{Path as AxumPath, State},
    Json,
};
use ra_params::{get_agent_metadata, models::ResourceAgent as RaParamsResourceAgent};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path as StdPath;
use std::sync::Arc;
use utoipa::ToSchema;
use indexmap::IndexMap;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Ordered parameter entry (key-value pair with order preserved)
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ParamEntry {
    pub key: String,
    pub value: String,
}

/// Flattened ResourceAgent metadata matching frontend all_agents.ts format
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ResourceAgent {
    pub name: String,
    pub version: String,
    pub shortdesc: String,
    pub longdesc: String,
    pub parameters: Vec<Parameter>,
    pub actions: Vec<Action>,
}

/// Flattened parameter matching frontend all_agents.ts format
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct Parameter {
    pub name: String,
    pub unique: bool,
    pub required: bool,
    pub shortdesc: String,
    pub longdesc: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub default: String,
}

/// Flattened action matching frontend all_agents.ts format
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct Action {
    pub name: String,
    pub timeout: String,
    pub interval: String,
    pub depth: String,
}

/// Convert from ra_params::ResourceAgent to flattened format
impl From<&RaParamsResourceAgent> for ResourceAgent {
    fn from(ra: &RaParamsResourceAgent) -> Self {
        ResourceAgent {
            name: ra.name.clone(),
            version: ra.version(),
            shortdesc: ra.shortdesc.text.clone(),
            longdesc: ra.longdesc.text.clone(),
            parameters: ra.parameters.parameters.iter().map(|p| Parameter {
                name: p.name.clone(),
                unique: p.unique == "1",
                required: p.required == "1",
                shortdesc: p.shortdesc.text.clone(),
                longdesc: p.longdesc.text.clone(),
                type_: p.content.type_.clone(),
                default: p.content.default.clone(),
            }).collect(),
            actions: ra.actions.actions.iter().map(|a| Action {
                name: a.name.clone(),
                timeout: a.timeout.clone(),
                interval: a.interval.clone(),
                depth: a.depth.clone(),
            }).collect(),
        }
    }
}

/// Parsed OCF agent string
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ParsedOcfAgent {
    /// The original string from TOML
    pub original: String,
    /// Provider (e.g., "heartbeat")
    pub provider: String,
    /// Agent type (e.g., "Filesystem")
    pub agent_type: String,
    /// Instance name (e.g., "fs_cluster_private")
    pub instance_name: String,
    /// Parameters extracted from the string
    /// Parameters extracted from the string (order preserved as array)
    pub params: Vec<ParamEntry>,
}

/// A generic item in the start/stop array
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct StartArrayItem {
    /// The original string from TOML
    pub original: String,
    /// Whether this is an OCF agent or a plain systemd unit
    pub is_ocf: bool,
    /// Parsed OCF agent info (if this is an OCF agent)
    pub ocf_agent: Option<ParsedOcfAgent>,
}

/// Structured TOML section
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TomlSection {
    /// Section name (e.g., "promoter")
    pub name: String,
    /// Whether this is an array section [[...]]
    pub is_array: bool,
    /// All key-value pairs in this section
    pub items: Vec<TomlItem>,
}

/// A TOML key-value pair
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TomlItem {
    /// The key
    pub key: String,
    /// The raw value
    pub value: String,
    /// Whether this value is an OCF agent string
    pub is_ocf_agent: bool,
    /// Parsed OCF agent info (if applicable)
    pub parsed_agent: Option<ParsedOcfAgent>,
}

/// Parsed TOML with OCF agent metadata
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TomlWithAgents {
    /// Parsed sections
    pub sections: Vec<TomlSection>,
    /// OCF agents with their metadata
    pub ocf_agents: Vec<OcfAgentWithMetadata>,
}

/// Start array item with metadata
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StartArrayItemWithMetadata {
    /// The item's position (start array index, stop array index, etc.)
    pub position: OcfAgentPosition,
    /// The item data (OCF agent or plain unit)
    pub item: StartArrayItem,
    /// Full metadata from OCF agent (if applicable, flattened format matching frontend all_agents.ts)
    pub metadata: Option<ResourceAgent>,
}

// Old alias for backward compatibility
pub type OcfAgentWithMetadata = StartArrayItemWithMetadata;

/// Position of an OCF agent in the configuration
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OcfAgentPosition {
    /// Section name (e.g., "promoter")
    pub section: String,
    /// Array index (if in an array)
    pub array_index: Option<usize>,
    /// Key name (e.g., "start" or "stop")
    pub key: String,
    /// Index in the start/stop array
    pub index: usize,
}

/// Response for parsed TOML with agent metadata
#[derive(Debug, Serialize, ToSchema)]
pub struct TomlWithAgentsResponse {
    /// Profile name
    pub profile: String,
    /// Path to the TOML file
    pub path: String,
    /// Parsed content with agents
    pub content: TomlWithAgents,
}

/// Parse OCF agent string from TOML array
/// Format: "ocf:provider:agent instance_name key=value key=value"
///
/// Example: "ocf:heartbeat:Filesystem fs_cluster_private device=/dev/drbd0 directory=/data fstype=ext4"
fn parse_ocf_agent_string(agent_str: &str) -> Option<ParsedOcfAgent> {
    let trimmed = agent_str.trim();

    if !trimmed.starts_with("ocf:") {
        return None;
    }

    // Remove "ocf:" prefix
    let without_prefix = &trimmed[4..];

    // Find the first space to separate "provider:agent instance_name" from params
    let first_space_idx = match without_prefix.find(' ') {
        Some(idx) => idx,
        None => {
            // No space - means just "ocf:provider:agent" with no instance name or params
            // Parse "provider:agent"
            let colon_idx = without_prefix.find(':')?;
            let provider = without_prefix[..colon_idx].to_string();
            let agent_type = without_prefix[colon_idx + 1..].to_string();

            return Some(ParsedOcfAgent {
                original: trimmed.to_string(),
                provider,
                agent_type: agent_type.clone(),
                instance_name: format!("{}_default", agent_type),
                params: Vec::new(),
            });
        }
    };

    let first_part = &without_prefix[..first_space_idx];
    let params_string = &without_prefix[first_space_idx + 1..];

    // Parse "provider:agent"
    let colon_idx = first_part.find(':')?;
    let provider = first_part[..colon_idx].to_string();
    let agent_type = first_part[colon_idx + 1..].to_string();

    // Split params string, handling quoted values like options='rw,all_squash,anonuid=0'
    // Use IndexMap internally to preserve insertion order
    let mut params_map: IndexMap<String, String> = IndexMap::new();
    let mut param_parts: Vec<String> = Vec::new();
    let mut current_param = String::new();
    let mut in_quotes = false;
    let mut quote_char = ' ';

    for (i, ch) in params_string.chars().enumerate() {
        if (ch == '"' || ch == '\'') && (i == 0 || params_string.as_bytes()[i - 1] != b'\\') {
            if !in_quotes {
                in_quotes = true;
                quote_char = ch;
                current_param.push(ch);
            } else if ch == quote_char {
                in_quotes = false;
                quote_char = ' ';
                current_param.push(ch);
            } else {
                current_param.push(ch);
            }
        } else if ch == ' ' && !in_quotes {
            if !current_param.trim().is_empty() {
                param_parts.push(current_param.trim().to_string());
                current_param = String::new();
            }
        } else {
            current_param.push(ch);
        }
    }
    if !current_param.trim().is_empty() {
        param_parts.push(current_param.trim().to_string());
    }

    // First param is the instance name, but check if it's a key=value pair
    let (instance_name, params_start_idx) = if param_parts.is_empty() {
        (format!("{}_default", agent_type), 0)
    } else {
        let first = &param_parts[0];
        if first.contains('=') {
            // First param is a key=value pair, no instance name provided
            (format!("{}_default", agent_type), 0)
        } else {
            // First param is the instance name
            (first.clone(), 1)
        }
    };

    // Remaining params are key=value pairs
    for part in &param_parts[params_start_idx..] {
        if let Some(eq_idx) = part.find('=') {
            let key = part[..eq_idx].to_string();
            let mut value = part[eq_idx + 1..].to_string();

            // Remove surrounding quotes if present
            if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                value = value[1..value.len() - 1].to_string();
            }

            params_map.insert(key, value);
        }
    }

    // Convert IndexMap to Vec<ParamEntry> to preserve order in JSON
    let params: Vec<ParamEntry> = params_map
        .iter()
        .map(|(k, v)| ParamEntry { key: k.clone(), value: v.clone() })
        .collect();

    Some(ParsedOcfAgent {
        original: trimmed.to_string(),
        provider,
        agent_type,
        instance_name,
        params,
    })
}

/// Parse a start array item (either OCF agent or plain systemd unit)
fn parse_start_array_item(item_str: &str) -> StartArrayItem {
    let trimmed = item_str.trim();

    // Try to parse as OCF agent first
    if let Some(ocf_agent) = parse_ocf_agent_string(trimmed) {
        return StartArrayItem {
            original: trimmed.to_string(),
            is_ocf: true,
            ocf_agent: Some(ocf_agent),
        };
    }

    // Not an OCF agent - treat as plain systemd unit
    StartArrayItem {
        original: trimmed.to_string(),
        is_ocf: false,
        ocf_agent: None,
    }
}

/// Parse TOML content and extract OCF agents
fn parse_toml_with_agents(content: &str) -> TomlWithAgents {
    let mut sections: Vec<TomlSection> = Vec::new();
    let mut ocf_agents: Vec<OcfAgentWithMetadata> = Vec::new();

    let lines: Vec<&str> = content.lines().collect();
    let mut current_section: Option<(String, bool, Vec<TomlItem>)> = None;

    let mut line_idx = 0;
    while line_idx < lines.len() {
        let line = lines[line_idx];
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            line_idx += 1;
            continue;
        }

        // Section header [section] or [[array]]
        if trimmed.starts_with('[') {
            // Save previous section
            if let Some((name, is_array, items)) = current_section.take() {
                sections.push(TomlSection {
                    name,
                    is_array,
                    items,
                });
            }

            if trimmed.starts_with("[[") && trimmed.ends_with("]]") {
                // Array section [[promoter]]
                let full_name = trimmed[2..trimmed.len() - 2].to_string();
                // Use only the last part as the section name (e.g., "promoter.resources.iscsi2" -> "iscsi2")
                let name = full_name.split('.').last().unwrap_or(&full_name).to_string();
                current_section = Some((name, true, Vec::new()));
            } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
                // Regular section [section]
                let full_name = trimmed[1..trimmed.len() - 1].to_string();
                // Use only the last part as the section name (e.g., "promoter.metadata" -> "metadata")
                let name = full_name.split('.').last().unwrap_or(&full_name).to_string();
                current_section = Some((name, false, Vec::new()));
            }
            line_idx += 1;
            continue;
        }

        // Key = Value
        if let Some(eq_idx) = line.find('=') {
            let key = line[..eq_idx].trim().to_string();
            let mut value_str = line[eq_idx + 1..].trim().to_string();

            // Check if this is the start of an array
            if value_str.starts_with('[') {
                let mut array_elements: Vec<String> = Vec::new();

                // Check if this is a single-line array (has closing bracket on same line)
                if value_str.contains(']') {
                    // Single-line array: ["a", "b", "c"]
                    // Extract content between [ and ]
                    let start_idx = value_str.find('[').unwrap() + 1;
                    let end_idx = value_str.rfind(']').unwrap();
                    let inner_content = &value_str[start_idx..end_idx];

                    // Split by commas and parse each element

                    // Parse comma-separated elements, handling quoted strings
                    let mut current = String::new();
                    let mut in_quotes = false;
                    let mut quote_char = ' ';

                    for ch in inner_content.chars() {
                        if (ch == '"' || ch == '\'') && (current.is_empty() || current.chars().last() != Some('\\')) {
                            if !in_quotes {
                                in_quotes = true;
                                quote_char = ch;
                                current.push(ch);
                            } else if ch == quote_char {
                                in_quotes = false;
                                quote_char = ' ';
                                current.push(ch);
                            } else {
                                current.push(ch);
                            }
                        } else if ch == ',' && !in_quotes {
                            let trimmed = current.trim().to_string();
                            if !trimmed.is_empty() {
                                // Remove surrounding quotes
                                let element = if (trimmed.starts_with('"') && trimmed.ends_with('"'))
                                    || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
                                {
                                    trimmed[1..trimmed.len() - 1].to_string()
                                } else {
                                    trimmed
                                };
                                array_elements.push(element);
                            }
                            current = String::new();
                        } else {
                            current.push(ch);
                        }
                    }

                    // Don't forget the last element
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        let element = if (trimmed.starts_with('"') && trimmed.ends_with('"'))
                            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
                        {
                            trimmed[1..trimmed.len() - 1].to_string()
                        } else {
                            trimmed
                        };
                        array_elements.push(element);
                    }

                    // Single-line array is complete, move to next line
                    line_idx += 1;
                } else {
                    // Multi-line array - collect all elements
                    let mut in_array = true;
                    let mut array_content = value_str.clone();

                    // Keep reading lines until we find the closing bracket
                    while in_array && line_idx < lines.len() {
                        // Check if this line has the closing bracket
                        if array_content.contains(']') {
                            // Extract content before the closing bracket
                            let closing_idx = array_content.find(']').unwrap();
                            let before_closing = array_content[..closing_idx].to_string();
                            array_content = before_closing;
                            in_array = false;
                        }

                        // Process the current content
                        let content_trimmed = array_content.trim();

                        if !content_trimmed.is_empty() && content_trimmed != "[" {
                            // Remove trailing comma if present
                            let content_clean = if content_trimmed.ends_with(',') {
                                &content_trimmed[..content_trimmed.len() - 1]
                            } else {
                                content_trimmed
                            };

                            // Remove quotes
                            let element_str = if (content_clean.starts_with('"') && content_clean.ends_with('"'))
                                || (content_clean.starts_with('\'') && content_clean.ends_with('\''))
                            {
                                content_clean[1..content_clean.len() - 1].to_string()
                            } else {
                                content_clean.to_string()
                            };

                            if !element_str.is_empty() {
                                array_elements.push(element_str);
                            }
                        }

                        line_idx += 1;
                        if line_idx < lines.len() && in_array {
                            array_content = lines[line_idx].to_string();
                        }
                    }
                }

                // Process array elements
                if key == "start" || key == "stop" {
                    // Store the array as a single item with all elements
                    let item = TomlItem {
                        key: key.clone(),
                        value: format!("[{}]", array_elements.join(", ")),
                        is_ocf_agent: false,
                        parsed_agent: None,
                    };

                    if let Some(ref mut section) = current_section {
                        section.2.push(item);
                    }

                    // Collect ALL items from the array (not just OCF agents)
                    for (idx, element) in array_elements.iter().enumerate() {
                        let start_item = parse_start_array_item(element);

                        if let Some(ref section) = current_section {
                            let (section_name, is_array, _) = section;

                            ocf_agents.push(OcfAgentWithMetadata {
                                position: OcfAgentPosition {
                                    section: section_name.clone(),
                                    array_index: if *is_array { Some(0) } else { None },
                                    key: key.clone(),
                                    index: idx,
                                },
                                item: start_item,
                                metadata: None,
                            });
                        }
                    }
                }
            } else {
                // Simple key = value (not an array)
                // Remove quotes from strings
                if (value_str.starts_with('"') && value_str.ends_with('"'))
                    || (value_str.starts_with('\'') && value_str.ends_with('\''))
                {
                    value_str = value_str[1..value_str.len() - 1].to_string();
                }

                // Check if this is an OCF agent string
                let is_ocf_agent = value_str.starts_with("ocf:");
                let parsed_agent = is_ocf_agent.then(|| parse_ocf_agent_string(&value_str)).flatten();

                let item = TomlItem {
                    key: key.clone(),
                    value: value_str.clone(),
                    is_ocf_agent,
                    parsed_agent: parsed_agent.clone(),
                };

                if let Some(ref mut section) = current_section {
                    section.2.push(item);
                }

                // Collect ALL items from start/stop arrays (single-line format)
                if key == "start" || key == "stop" {
                    if let Some(ref section) = current_section {
                        let (section_name, is_array, _) = section;

                        let start_item = parse_start_array_item(&value_str);

                        ocf_agents.push(OcfAgentWithMetadata {
                            position: OcfAgentPosition {
                                section: section_name.clone(),
                                array_index: if *is_array { Some(0) } else { None },
                                key: key.clone(),
                                index: ocf_agents.len(),
                            },
                            item: start_item,
                            metadata: None,
                        });
                    }
                }

                line_idx += 1;
            }
        } else {
            line_idx += 1;
        }
    }

    // Don't forget the last section
    if let Some((name, is_array, items)) = current_section {
        sections.push(TomlSection {
            name,
            is_array,
            items,
        });
    }

    TomlWithAgents {
        sections,
        ocf_agents,
    }
}

/// GET /api/v1/ha/profiles/{id}/toml/parse
///
/// Parse TOML configuration and extract OCF agents with metadata
#[utoipa::path(
    get,
    path = "/api/v1/ha/profiles/{id}/toml/parse",
    tag = "HA",
    params(
        ("id" = String, Path, description = "Profile ID or name")
    ),
    responses(
        (status = 200, description = "Parsed TOML with OCF agent metadata", body = TomlWithAgentsResponse),
        (status = 404, description = "TOML file not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn parse_profile_toml(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> AppResult<Json<TomlWithAgentsResponse>> {
    let config_path = state.reactor_config_path(&id);

    // Check if the TOML file exists
    if !StdPath::new(&config_path).exists() {
        return Err(AppError::NotFound(format!(
            "TOML configuration file not found for profile '{}': {}",
            id, config_path
        )));
    }

    // Read the TOML content
    let content = fs::read_to_string(&config_path).map_err(|e| {
        AppError::Internal(format!("Failed to read TOML file '{}': {}", config_path, e))
    })?;

    // Parse TOML and extract OCF agents
    let mut parsed = parse_toml_with_agents(&content);

    // Fetch metadata for each OCF agent
    for agent_with_meta in &mut parsed.ocf_agents {
        // Only fetch metadata for OCF agents
        let ocf_agent = match &agent_with_meta.item.ocf_agent {
            Some(agent) => agent,
            None => continue,
        };

        let agent_path = StdPath::new("/usr/lib/ocf")
            .join("resource.d")
            .join(&ocf_agent.provider)
            .join(&ocf_agent.agent_type);

        if agent_path.exists() {
            match get_agent_metadata(&agent_path) {
                Ok((ra_metadata, _)) => {
                    // Convert to flattened format
                    let metadata: ResourceAgent = ResourceAgent::from(&ra_metadata);
                    agent_with_meta.metadata = Some(metadata);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to get metadata for {}/{}: {}",
                        ocf_agent.provider,
                        ocf_agent.agent_type,
                        e
                    );
                }
            }
        }
    }

    Ok(Json(TomlWithAgentsResponse {
        profile: id,
        path: config_path,
        content: parsed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to get param value by key from Vec<ParamEntry>
    fn get_param(params: &[ParamEntry], key: &str) -> Option<&str> {
        params.iter().find(|p| p.key == key).map(|p| p.value.as_str())
    }

    #[test]
    fn test_parse_simple_ocf_agent_string() {
        let agent_str = "ocf:heartbeat:Filesystem fs_cluster_private device=/dev/drbd0 directory=/data fstype=ext4";
        let result = parse_ocf_agent_string(agent_str);

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(agent.provider, "heartbeat");
        assert_eq!(agent.agent_type, "Filesystem");
        assert_eq!(agent.instance_name, "fs_cluster_private");
        assert_eq!(get_param(&agent.params, "device"), Some(&"/dev/drbd0".to_string()));
        assert_eq!(get_param(&agent.params, "directory"), Some(&"/data".to_string()));
        assert_eq!(get_param(&agent.params, "fstype"), Some(&"ext4".to_string()));
        assert_eq!(agent.params.len(), 3);
    }

    #[test]
    fn test_parse_ocf_agent_with_quoted_params() {
        let agent_str = "ocf:heartbeat:Filesystem fs_cluster_private device=/dev/drbd0 directory=/data fstype=ext4 options='rw,all_squash,anonuid=0'";
        let result = parse_ocf_agent_string(agent_str);

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(agent.provider, "heartbeat");
        assert_eq!(agent.agent_type, "Filesystem");
        assert_eq!(agent.instance_name, "fs_cluster_private");
        assert_eq!(get_param(&agent.params, "options"), Some(&"rw,all_squash,anonuid=0".to_string()));
    }

    #[test]
    fn test_parse_ocf_agent_with_double_quoted_params() {
        let agent_str = r#"ocf:heartbeat:Filesystem fs_cluster_private device=/dev/drbd0 directory="/path with spaces" fstype=ext4"#;
        let result = parse_ocf_agent_string(agent_str);

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(get_param(&agent.params, "directory"), Some(&"/path with spaces".to_string()));
    }

    #[test]
    fn test_parse_ocf_ipaddr2() {
        let agent_str = "ocf:heartbeat:IPaddr2 service_ip0 cidr_netmask=24 ip=192.168.123.191";
        let result = parse_ocf_agent_string(agent_str);

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(agent.provider, "heartbeat");
        assert_eq!(agent.agent_type, "IPaddr2");
        assert_eq!(agent.instance_name, "service_ip0");
        assert_eq!(get_param(&agent.params, "cidr_netmask"), Some(&"24".to_string()));
        assert_eq!(get_param(&agent.params, "ip"), Some(&"192.168.123.191".to_string()));
    }

    #[test]
    fn test_parse_ocf_portblock() {
        let agent_str = "ocf:heartbeat:portblock pblock0 action=block ip=192.168.123.191 portno=3260 protocol=tcp";
        let result = parse_ocf_agent_string(agent_str);

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(agent.provider, "heartbeat");
        assert_eq!(agent.agent_type, "portblock");
        assert_eq!(agent.instance_name, "pblock0");
        assert_eq!(get_param(&agent.params, "action"), Some(&"block".to_string()));
        assert_eq!(get_param(&agent.params, "ip"), Some(&"192.168.123.191".to_string()));
        assert_eq!(get_param(&agent.params, "portno"), Some(&"3260".to_string()));
        assert_eq!(get_param(&agent.params, "protocol"), Some(&"tcp".to_string()));
    }

    #[test]
    fn test_parse_ocf_iscsi_target() {
        let agent_str = "ocf:heartbeat:iSCSITarget target allowed_initiators='' incoming_password='' incoming_username='' iqn=iqn.2025-12.com.linbit:iscsi2 portals=192.168.123.191:3260";
        let result = parse_ocf_agent_string(agent_str);

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(agent.provider, "heartbeat");
        assert_eq!(agent.agent_type, "iSCSITarget");
        assert_eq!(agent.instance_name, "target");
        assert_eq!(get_param(&agent.params, "allowed_initiators"), Some(&"".to_string()));
        assert_eq!(get_param(&agent.params, "iqn"), Some(&"iqn.2025-12.com.linbit:iscsi2".to_string()));
        assert_eq!(get_param(&agent.params, "portals"), Some(&"192.168.123.191:3260".to_string()));
    }

    #[test]
    fn test_parse_ocf_iscsi_logical_unit() {
        let agent_str = "ocf:heartbeat:iSCSILogicalUnit lu1 lun=1 path=/dev/drbd/by-res/iscsi2/1 product_id=d30d7c86 scsi_sn=d30d7c86 target_iqn=iqn.2025-12.com.linbit:iscsi2";
        let result = parse_ocf_agent_string(agent_str);

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(agent.provider, "heartbeat");
        assert_eq!(agent.agent_type, "iSCSILogicalUnit");
        assert_eq!(agent.instance_name, "lu1");
        assert_eq!(get_param(&agent.params, "lun"), Some(&"1".to_string()));
        assert_eq!(get_param(&agent.params, "path"), Some(&"/dev/drbd/by-res/iscsi2/1".to_string()));
        assert_eq!(get_param(&agent.params, "product_id"), Some(&"d30d7c86".to_string()));
        assert_eq!(get_param(&agent.params, "scsi_sn"), Some(&"d30d7c86".to_string()));
        assert_eq!(get_param(&agent.params, "target_iqn"), Some(&"iqn.2025-12.com.linbit:iscsi2".to_string()));
    }

    #[test]
    fn test_parse_non_ocf_string() {
        let agent_str = "not an ocf string";
        let result = parse_ocf_agent_string(agent_str);

        assert!(result.is_none());
    }

    #[test]
    fn test_parse_simple_toml() {
        let toml_content = r#"
[[promoter]]

  [promoter.resources.test]
    runner = "systemd"
    start = [
      "ocf:heartbeat:Filesystem fs device=/dev/drbd0 directory=/data fstype=ext4",
    ]
"#;

        let result = parse_toml_with_agents(toml_content);

        assert!(!result.sections.is_empty());
        assert!(!result.ocf_agents.is_empty());

        let agent = &result.ocf_agents[0];
        assert_eq!(agent.item.ocf_agent.as_ref().unwrap().provider, "heartbeat");
        assert_eq!(agent.item.ocf_agent.as_ref().unwrap().agent_type, "Filesystem");
        assert_eq!(agent.item.ocf_agent.as_ref().unwrap().instance_name, "fs");
        assert_eq!(agent.position.key, "start");
    }

    #[test]
    fn test_parse_linstor_gateway_toml() {
        let toml_content = r#"# Generated by LINSTOR Gateway at 2025-12-23 08:16:02.267771316 +0000 UTC m=+2021.255277057
# DO NOT MODIFY!

[[promoter]]

  [promoter.metadata]
    linstor-gateway-schema-version = 1

  [promoter.resources]

    [promoter.resources.iscsi2]
      on-drbd-demote-failure = "reboot-immediate"
      runner = "systemd"
      start = [
        "ocf:heartbeat:Filesystem fs_cluster_private device=/dev/drbd/by-res/iscsi2/0 directory=/srv/ha/internal/iscsi2 fstype=ext4 run_fsck=no",
        "ocf:heartbeat:portblock pblock0 action=block ip=192.168.123.191 portno=3260 protocol=tcp",
        "ocf:heartbeat:IPaddr2 service_ip0 cidr_netmask=24 ip=192.168.123.191",
        "ocf:heartbeat:iSCSITarget target allowed_initiators='' incoming_password='' incoming_username='' iqn=iqn.2025-12.com.linbit:iscsi2 portals=192.168.123.191:3260",
        "ocf:heartbeat:iSCSILogicalUnit lu1 lun=1 path=/dev/drbd/by-res/iscsi2/1 product_id=d30d7c86 scsi_sn=d30d7c86 target_iqn=iqn.2025-12.com.linbit:iscsi2",
        "ocf:heartbeat:iSCSILogicalUnit lu2 lun=2 path=/dev/drbd/by-res/iscsi2/2 product_id=bbc9c739 scsi_sn=bbc9c739 target_iqn=iqn.2025-12.com.linbit:iscsi2",
        "ocf:heartbeat:iSCSILogicalUnit lu3 lun=3 path=/dev/drbd/by-res/iscsi2/3 product_id=c1b2695b scsi_sn=c1b2695b target_iqn=iqn.2025-12.com.linbit:iscsi2",
        "ocf:heartbeat:portblock portunblock0 action=unblock ip=192.168.123.191 portno=3260 protocol=tcp tickle_dir=/srv/ha/internal/iscsi2",
      ]
      stop-services-on-exit = true
      target-as = "Requires"
"#;

        let result = parse_toml_with_agents(toml_content);

        // Should find the promoter section
        assert!(!result.sections.is_empty());
        let promoter_section = result.sections.iter().find(|s| s.name == "promoter");
        assert!(promoter_section.is_some());

        // Should find exactly 8 OCF agents (all from start array)
        assert_eq!(result.ocf_agents.len(), 8);

        // Verify first agent: Filesystem
        let fs_agent = &result.ocf_agents[0];
        assert_eq!(fs_agent.item.ocf_agent.as_ref().unwrap().provider, "heartbeat");
        assert_eq!(fs_agent.item.ocf_agent.as_ref().unwrap().agent_type, "Filesystem");
        assert_eq!(fs_agent.item.ocf_agent.as_ref().unwrap().instance_name, "fs_cluster_private");
        assert_eq!(fs_agent.item.ocf_agent.as_ref().unwrap().params.get("device"), Some(&"/dev/drbd/by-res/iscsi2/0".to_string()));
        assert_eq!(fs_agent.item.ocf_agent.as_ref().unwrap().params.get("directory"), Some(&"/srv/ha/internal/iscsi2".to_string()));
        assert_eq!(fs_agent.item.ocf_agent.as_ref().unwrap().params.get("fstype"), Some(&"ext4".to_string()));
        assert_eq!(fs_agent.item.ocf_agent.as_ref().unwrap().params.get("run_fsck"), Some(&"no".to_string()));
        assert_eq!(fs_agent.position.section, "iscsi2");  // Changed from "promoter" to "iscsi2"
        assert_eq!(fs_agent.position.key, "start");
        assert_eq!(fs_agent.position.index, 0);

        // Verify second agent: portblock (pblock0)
        let pblock0 = &result.ocf_agents[1];
        assert_eq!(pblock0.item.ocf_agent.as_ref().unwrap().agent_type, "portblock");
        assert_eq!(pblock0.item.ocf_agent.as_ref().unwrap().instance_name, "pblock0");
        assert_eq!(pblock0.item.ocf_agent.as_ref().unwrap().params.get("action"), Some(&"block".to_string()));
        assert_eq!(pblock0.position.index, 1);

        // Verify third agent: IPaddr2
        let ipaddr2 = &result.ocf_agents[2];
        assert_eq!(ipaddr2.item.ocf_agent.as_ref().unwrap().agent_type, "IPaddr2");
        assert_eq!(ipaddr2.item.ocf_agent.as_ref().unwrap().instance_name, "service_ip0");
        assert_eq!(ipaddr2.item.ocf_agent.as_ref().unwrap().params.get("ip"), Some(&"192.168.123.191".to_string()));
        assert_eq!(ipaddr2.position.index, 2);

        // Verify fourth agent: iSCSITarget
        let target = &result.ocf_agents[3];
        assert_eq!(target.item.ocf_agent.as_ref().unwrap().agent_type, "iSCSITarget");
        assert_eq!(target.item.ocf_agent.as_ref().unwrap().instance_name, "target");
        assert_eq!(target.item.ocf_agent.as_ref().unwrap().params.get("iqn"), Some(&"iqn.2025-12.com.linbit:iscsi2".to_string()));
        assert_eq!(target.position.index, 3);

        // Verify fifth agent: iSCSILogicalUnit lu1
        let lu1 = &result.ocf_agents[4];
        assert_eq!(lu1.item.ocf_agent.as_ref().unwrap().agent_type, "iSCSILogicalUnit");
        assert_eq!(lu1.item.ocf_agent.as_ref().unwrap().instance_name, "lu1");
        assert_eq!(lu1.item.ocf_agent.as_ref().unwrap().params.get("lun"), Some(&"1".to_string()));
        assert_eq!(lu1.item.ocf_agent.as_ref().unwrap().params.get("path"), Some(&"/dev/drbd/by-res/iscsi2/1".to_string()));
        assert_eq!(lu1.position.index, 4);

        // Verify sixth agent: iSCSILogicalUnit lu2
        let lu2 = &result.ocf_agents[5];
        assert_eq!(lu2.item.ocf_agent.as_ref().unwrap().instance_name, "lu2");
        assert_eq!(lu2.item.ocf_agent.as_ref().unwrap().params.get("lun"), Some(&"2".to_string()));
        assert_eq!(lu2.item.ocf_agent.as_ref().unwrap().params.get("path"), Some(&"/dev/drbd/by-res/iscsi2/2".to_string()));
        assert_eq!(lu2.position.index, 5);

        // Verify seventh agent: iSCSILogicalUnit lu3
        let lu3 = &result.ocf_agents[6];
        assert_eq!(lu3.item.ocf_agent.as_ref().unwrap().instance_name, "lu3");
        assert_eq!(lu3.item.ocf_agent.as_ref().unwrap().params.get("lun"), Some(&"3".to_string()));
        assert_eq!(lu3.item.ocf_agent.as_ref().unwrap().params.get("path"), Some(&"/dev/drbd/by-res/iscsi2/3".to_string()));
        assert_eq!(lu3.position.index, 6);

        // Verify eighth agent: portblock (portunblock0)
        let portunblock0 = &result.ocf_agents[7];
        assert_eq!(portunblock0.item.ocf_agent.as_ref().unwrap().agent_type, "portblock");
        assert_eq!(portunblock0.item.ocf_agent.as_ref().unwrap().instance_name, "portunblock0");
        assert_eq!(portunblock0.item.ocf_agent.as_ref().unwrap().params.get("action"), Some(&"unblock".to_string()));
        assert_eq!(portunblock0.item.ocf_agent.as_ref().unwrap().params.get("tickle_dir"), Some(&"/srv/ha/internal/iscsi2".to_string()));
        assert_eq!(portunblock0.position.index, 7);

        // Verify that the metadata section was parsed
        let metadata_section = result.sections.iter()
            .find(|s| s.name == "metadata");  // Changed from "promoter.metadata" to "metadata"
        assert!(metadata_section.is_some());

        // Verify that non-OCF items were parsed correctly
        let metadata_items = metadata_section.unwrap();
        assert!(metadata_items.items.iter()
            .any(|item| item.key == "linstor-gateway-schema-version" && item.value == "1"));
    }

    #[test]
    fn test_parse_toml_with_comments() {
        let toml_content = r#"
# This is a comment
[[promoter]]
    # Another comment
    [promoter.resources]
        # Yet another comment
        [promoter.resources.test]
          runner = "systemd"
"#;

        let result = parse_toml_with_agents(toml_content);
        assert!(!result.sections.is_empty());
    }

    #[test]
    fn test_parse_toml_empty_start_array() {
        let toml_content = r#"
[[promoter]]
  [promoter.resources.test]
    runner = "systemd"
    start = []
"#;

        let result = parse_toml_with_agents(toml_content);
        // Should parse successfully but with no OCF agents
        assert_eq!(result.ocf_agents.len(), 0);
    }

    #[test]
    fn test_parse_ocf_agent_without_params() {
        let agent_str = "ocf:heartbeat:IPaddr2 test_ip";
        let result = parse_ocf_agent_string(agent_str);

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(agent.provider, "heartbeat");
        assert_eq!(agent.agent_type, "IPaddr2");
        assert_eq!(agent.instance_name, "test_ip");
        assert_eq!(agent.params.len(), 0);
    }

    #[test]
    fn test_parse_ocf_agent_with_empty_instance_name() {
        let agent_str = "ocf:heartbeat:Filesystem device=/dev/drbd0";
        let result = parse_ocf_agent_string(agent_str);

        assert!(result.is_some());
        let agent = result.unwrap();
        // When no instance name is provided, should use default
        assert_eq!(agent.instance_name, "Filesystem_default");
        assert_eq!(get_param(&agent.params, "device"), Some(&"/dev/drbd0".to_string()));
    }

    #[test]
    fn test_parse_exportfs_with_quoted_params() {
        let agent_str = "ocf:heartbeat:exportfs export_1_0 clientspec=0.0.0.0/0.0.0.0 directory=/srv/gateway-exports/mynfs fsid=199b431f-c4c3-5eb0-ab46-264e8261ad34 options='rw,all_squash,anonuid=0,anongid=0'";
        let result = parse_ocf_agent_string(agent_str);

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(agent.provider, "heartbeat");
        assert_eq!(agent.agent_type, "exportfs");
        assert_eq!(agent.instance_name, "export_1_0");
        assert_eq!(get_param(&agent.params, "clientspec"), Some(&"0.0.0.0/0.0.0.0".to_string()));
        assert_eq!(get_param(&agent.params, "directory"), Some(&"/srv/gateway-exports/mynfs".to_string()));
        assert_eq!(get_param(&agent.params, "fsid"), Some(&"199b431f-c4c3-5eb0-ab46-264e8261ad34".to_string()));
        assert_eq!(get_param(&agent.params, "options"), Some(&"rw,all_squash,anonuid=0,anongid=0".to_string()));
    }

    #[test]
    fn test_parse_empty_string_params() {
        let agent_str = "ocf:heartbeat:iSCSITarget target allowed_initiators='' incoming_password='' incoming_username='' iqn=iqn.2025-12.com.linbit:iscsi2 portals=192.168.123.191:3260";
        let result = parse_ocf_agent_string(agent_str);

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(agent.provider, "heartbeat");
        assert_eq!(agent.agent_type, "iSCSITarget");
        assert_eq!(agent.instance_name, "target");
        assert_eq!(get_param(&agent.params, "allowed_initiators"), Some(&"".to_string()));
        assert_eq!(get_param(&agent.params, "incoming_password"), Some(&"".to_string()));
        assert_eq!(get_param(&agent.params, "incoming_username"), Some(&"".to_string()));
        assert_eq!(get_param(&agent.params, "iqn"), Some(&"iqn.2025-12.com.linbit:iscsi2".to_string()));
        assert_eq!(get_param(&agent.params, "portals"), Some(&"192.168.123.191:3260".to_string()));
    }

    #[test]
    fn test_parse_mynfs_toml() {
        let toml_content = r#"
[[promoter]]

  [promoter.resources.mynfs]
    runner = "systemd"
    start = [
      "ocf:heartbeat:portblock portblock action=block ip=192.168.123.192 portno=2049 protocol=tcp",
      "ocf:heartbeat:Filesystem fs_cluster_private device=/dev/drbd/by-res/mynfs/0 directory=/srv/ha/internal/mynfs fstype=ext4 run_fsck=no",
      "ocf:heartbeat:Filesystem fs_1 device=/dev/drbd/by-res/mynfs/1 directory=/srv/gateway-exports/mynfs fstype=ext4 run_fsck=no",
      "ocf:heartbeat:IPaddr2 service_ip cidr_netmask=24 ip=192.168.123.192",
      "ocf:heartbeat:nfsserver nfsserver nfs_ip=192.168.123.192 nfs_server_scope=192.168.123.192 nfs_shared_infodir=/srv/ha/internal/mynfs/nfs",
      "ocf:heartbeat:exportfs export_1_0 clientspec=0.0.0.0/0.0.0.0 directory=/srv/gateway-exports/mynfs fsid=199b431f-c4c3-5eb0-ab46-264e8261ad34 options='rw,all_squash,anonuid=0,anongid=0'",
      "ocf:heartbeat:portblock portunblock action=unblock ip=192.168.123.192 portno=2049 protocol=tcp tickle_dir=/srv/ha/internal/mynfs",
    ]
"#;

        let result = parse_toml_with_agents(toml_content);

        // Should find exactly 7 OCF agents
        assert_eq!(result.ocf_agents.len(), 7);

        // Verify first agent: portblock
        let pblock = &result.ocf_agents[0];
        assert!(pblock.item.is_ocf);
        assert_eq!(pblock.item.ocf_agent.as_ref().unwrap().agent_type, "portblock");
        assert_eq!(pblock.item.ocf_agent.as_ref().unwrap().instance_name, "portblock");
        assert_eq!(pblock.position.index, 0);

        // Verify second agent: Filesystem fs_cluster_private
        let fs1 = &result.ocf_agents[1];
        assert!(fs1.item.is_ocf);
        assert_eq!(fs1.item.ocf_agent.as_ref().unwrap().agent_type, "Filesystem");
        assert_eq!(fs1.item.ocf_agent.as_ref().unwrap().instance_name, "fs_cluster_private");
        assert_eq!(fs1.position.index, 1);

        // Verify third agent: Filesystem fs_1
        let fs2 = &result.ocf_agents[2];
        assert!(fs2.item.is_ocf);
        assert_eq!(fs2.item.ocf_agent.as_ref().unwrap().agent_type, "Filesystem");
        assert_eq!(fs2.item.ocf_agent.as_ref().unwrap().instance_name, "fs_1");
        assert_eq!(fs2.position.index, 2);

        // Verify sixth agent: exportfs with quoted options
        let exportfs = &result.ocf_agents[5];
        assert!(exportfs.item.is_ocf);
        assert_eq!(exportfs.item.ocf_agent.as_ref().unwrap().agent_type, "exportfs");
        assert_eq!(exportfs.item.ocf_agent.as_ref().unwrap().instance_name, "export_1_0");
        assert_eq!(exportfs.item.ocf_agent.as_ref().unwrap().params.get("options"), Some(&"rw,all_squash,anonuid=0,anongid=0".to_string()));
        assert_eq!(exportfs.position.index, 5);

        // Verify all agents are in "mynfs" section
        for agent in &result.ocf_agents {
            assert_eq!(agent.position.section, "mynfs");
            assert_eq!(agent.position.key, "start");
        }
    }

    #[test]
    fn test_parse_linstor_controller_toml() {
        let toml_content = r#"
# drbd-reactor promoter configuration
# Generated by drbd-ha-manager

[[promoter]]
id = "linstor_controller"

[promoter.resources.linstor_db]
start = ["var-lib-linstor.mount", "ocf:heartbeat:IPaddr2 service_ip cidr_netmask=24 ip=192.168.123.200", "linstor-controller.service", "frpc.service"]
runner = "systemd"
stop-services-on-exit = true
on-drbd-demote-failure = "reboot"
"#;

        let result = parse_toml_with_agents(toml_content);

        // Should find exactly 4 items in start array
        assert_eq!(result.ocf_agents.len(), 4);

        // First item: var-lib-linstor.mount (not OCF)
        let item0 = &result.ocf_agents[0];
        assert_eq!(item0.position.key, "start");
        assert_eq!(item0.position.index, 0);
        assert_eq!(item0.item.is_ocf, false);
        assert_eq!(item0.item.original, "var-lib-linstor.mount");

        // Second item: OCF IPaddr2
        let item1 = &result.ocf_agents[1];
        assert_eq!(item1.position.key, "start");
        assert_eq!(item1.position.index, 1);
        assert_eq!(item1.item.is_ocf, true);
        assert!(item1.item.ocf_agent.is_some());
        let ocf1 = item1.item.ocf_agent.as_ref().unwrap();
        assert_eq!(ocf1.provider, "heartbeat");
        assert_eq!(ocf1.agent_type, "IPaddr2");
        assert_eq!(ocf1.instance_name, "service_ip");
        assert_eq!(ocf1get_param(&.params, "cidr_netmask"), Some(&"24".to_string()));
        assert_eq!(ocf1get_param(&.params, "ip"), Some(&"192.168.123.200".to_string()));

        // Third item: linstor-controller.service (not OCF)
        let item2 = &result.ocf_agents[2];
        assert_eq!(item2.position.key, "start");
        assert_eq!(item2.position.index, 2);
        assert_eq!(item2.item.is_ocf, false);
        assert_eq!(item2.item.original, "linstor-controller.service");

        // Fourth item: frpc.service (not OCF)
        let item3 = &result.ocf_agents[3];
        assert_eq!(item3.position.key, "start");
        assert_eq!(item3.position.index, 3);
        assert_eq!(item3.item.is_ocf, false);
        assert_eq!(item3.item.original, "frpc.service");
    }
}

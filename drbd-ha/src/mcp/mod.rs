//! MCP (Model Context Protocol) server for DRBD-HA.
//!
//! Exposes the existing REST handlers as MCP tools over the streamable-HTTP
//! transport (mounted at `/mcp` in the main router) so AI agents can operate
//! the cluster. The bundled skills (see `skills/drbd-ha-ops/`) are exposed as
//! MCP prompts, giving any connected agent the operational playbooks.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use rmcp::{
    handler::server::router::{prompt::PromptRouter, tool::ToolRouter},
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, Content, GetPromptRequestParams, GetPromptResult, ListPromptsResult,
        PaginatedRequestParams, PromptMessage, PromptMessageRole, ServerCapabilities, ServerInfo,
    },
    prompt, prompt_handler, prompt_router,
    service::RequestContext,
    tool, tool_handler, tool_router, ErrorData as McpError, RoleServer, ServerHandler,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::api::{cluster, dashboard, ha, resource, storage};
use crate::error::AppError;
use crate::models::{AddNodeRequest, CreateHaProfileRequest, ResourceActionRequest};
use crate::state::AppState;

/// Convert an application error into an MCP error.
fn app_err(e: AppError) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

/// Wrap a serializable value as a successful JSON tool result.
fn ok_json<T: serde::Serialize>(value: T) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::json(value)?]))
}

// ---- Tool parameter types -------------------------------------------------

#[derive(Deserialize, JsonSchema)]
pub struct AddNodeParams {
    /// Node hostname
    pub hostname: String,
    /// Node IP address
    pub ip: String,
    /// SSH port (default 22)
    pub ssh_port: Option<u16>,
    /// SSH username (defaults to the configured global default)
    pub ssh_user: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct NodeIdParam {
    /// Node ID or hostname
    pub node_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ResourceNameParam {
    /// DRBD resource name
    pub name: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ResourceLogsParams {
    /// DRBD resource name
    pub name: String,
    /// Number of log lines (default 100, max 1000)
    pub lines: Option<u32>,
    /// Only logs since this time (e.g. "1h", "30m", "2024-01-15")
    pub since: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ResourceActionParams {
    /// DRBD resource name
    pub name: String,
    /// Action: up, down, primary, secondary, connect, disconnect, invalidate,
    /// verify, or recover_split_brain (DESTRUCTIVE: discards local data)
    pub action: String,
    /// Force the action (e.g. primary --force). Dangerous on partitioned clusters.
    #[serde(default)]
    pub force: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct ProfileIdParam {
    /// HA profile ID or name
    pub id_or_name: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct EvictParams {
    /// HA profile ID or name
    pub id_or_name: String,
    /// Node hostname/ID to evict from (defaults to the profile's current Primary)
    pub node: Option<String>,
    /// Seconds to wait for peer takeover (default 20)
    pub delay: Option<u32>,
    /// Keep the node masked after eviction (prevents automatic failback)
    #[serde(default)]
    pub keep_masked: bool,
    /// Force eviction even with warnings
    #[serde(default)]
    pub force: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct DeleteProfileParams {
    /// HA profile ID or name
    pub id_or_name: String,
    /// Also delete the associated DRBD resource (DESTRUCTIVE)
    #[serde(default)]
    pub delete_resource: bool,
    /// Also delete the promoter .toml from /etc/drbd-reactor.d/
    #[serde(default)]
    pub delete_config_file: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct ReactorReloadParams {
    /// "reload" (default) or "restart"
    pub action: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ReactorLogsParams {
    /// Number of log lines (default 100, max 1000)
    pub lines: Option<u32>,
    /// Only logs since this time (e.g. "1h", "30m")
    pub since: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ListServicesParams {
    /// Include system services (default false)
    #[serde(default)]
    pub include_system: bool,
}

// ---- MCP service ----------------------------------------------------------

#[derive(Clone)]
pub struct DrbdHaMcp {
    state: Arc<AppState>,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

impl DrbdHaMcp {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    fn st(&self) -> State<Arc<AppState>> {
        State(self.state.clone())
    }
}

#[tool_router]
impl DrbdHaMcp {
    // -- Discovery / health --

    #[tool(description = "Cluster health check: backend version and basic liveness")]
    async fn health(&self) -> Result<CallToolResult, McpError> {
        let Json(v) = cluster::health_check(self.st()).await;
        ok_json(v)
    }

    #[tool(
        description = "Dashboard summary: node/resource/profile counts and degraded items overview"
    )]
    async fn dashboard_summary(&self) -> Result<CallToolResult, McpError> {
        let Json(v) = dashboard::get_summary(self.st()).await.map_err(app_err)?;
        ok_json(v)
    }

    #[tool(description = "List all cluster nodes with status (online/offline/error)")]
    async fn list_nodes(&self) -> Result<CallToolResult, McpError> {
        let Json(v) = cluster::list_nodes(self.st()).await.map_err(app_err)?;
        ok_json(v)
    }

    #[tool(
        description = "Register a new cluster node. Key-based SSH from the controller to the node must already work; the node is probed on add."
    )]
    async fn add_node(
        &self,
        Parameters(p): Parameters<AddNodeParams>,
    ) -> Result<CallToolResult, McpError> {
        let req = AddNodeRequest {
            hostname: p.hostname,
            ip: p.ip,
            ssh_port: p.ssh_port,
            ssh_user: p.ssh_user,
        };
        let (_status, Json(v)) = cluster::add_node(self.st(), Json(req))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    #[tool(description = "Probe a node over SSH and refresh its status")]
    async fn check_node(
        &self,
        Parameters(p): Parameters<NodeIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let Json(v) = cluster::check_node_status(self.st(), Path(p.node_id))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    #[tool(
        description = "List block devices on a node that are available for DRBD/LVM (unmounted, no partitions)"
    )]
    async fn list_available_disks(
        &self,
        Parameters(p): Parameters<NodeIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let Json(v) = cluster::list_available_disks(self.st(), Path(p.node_id))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    #[tool(description = "List LVM/ZFS storage pools across managed nodes")]
    async fn list_pools(&self) -> Result<CallToolResult, McpError> {
        let Json(v) = storage::list_pools(self.st()).await.map_err(app_err)?;
        ok_json(v)
    }

    // -- DRBD resources --

    #[tool(description = "List all DRBD resources with role/disk/connection states")]
    async fn list_resources(&self) -> Result<CallToolResult, McpError> {
        let Json(v) = resource::list_resources(self.st()).await.map_err(app_err)?;
        ok_json(v)
    }

    #[tool(
        description = "Get one DRBD resource's detailed status (role, disk states, peers, sync progress)"
    )]
    async fn get_resource(
        &self,
        Parameters(p): Parameters<ResourceNameParam>,
    ) -> Result<CallToolResult, McpError> {
        let Json(v) = resource::get_resource(self.st(), Path(p.name))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    #[tool(description = "Get journal logs for a DRBD resource's promoter services")]
    async fn get_resource_logs(
        &self,
        Parameters(p): Parameters<ResourceLogsParams>,
    ) -> Result<CallToolResult, McpError> {
        let query = resource::LogsQuery {
            lines: p.lines.unwrap_or(100),
            since: p.since,
        };
        let Json(v) = resource::get_resource_logs(Path(p.name), Query(query))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    #[tool(
        description = "Run a drbdadm action on a resource: up/down/primary/secondary/connect/disconnect/invalidate/verify/recover_split_brain. DESTRUCTIVE actions (down, invalidate, recover_split_brain, force primary) need explicit user confirmation."
    )]
    async fn resource_action(
        &self,
        Parameters(p): Parameters<ResourceActionParams>,
    ) -> Result<CallToolResult, McpError> {
        let action =
            serde_json::from_value(serde_json::Value::String(p.action.clone())).map_err(|_| {
                McpError::invalid_params(format!("unknown action '{}'", p.action), None)
            })?;
        let req = ResourceActionRequest {
            action,
            force: p.force,
        };
        let Json(v) = resource::resource_action(self.st(), Path(p.name), Json(req))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    // -- HA profiles --

    #[tool(description = "List all HA profiles with status and current Primary node")]
    async fn list_ha_profiles(&self) -> Result<CallToolResult, McpError> {
        let Json(v) = ha::list_profiles(self.st()).await.map_err(app_err)?;
        ok_json(v)
    }

    #[tool(description = "Get one HA profile's full detail (services, VIP, nodes, status)")]
    async fn get_ha_profile(
        &self,
        Parameters(p): Parameters<ProfileIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let Json(v) = ha::get_profile(self.st(), Path(p.id_or_name))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    #[tool(
        description = "Refresh and get an HA profile's live runtime status (per-node enabled state, Primary, plugin health)"
    )]
    async fn get_ha_profile_status(
        &self,
        Parameters(p): Parameters<ProfileIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let Json(v) = ha::get_profile_status(self.st(), Path(p.id_or_name))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    #[tool(description = "Get the raw drbd-reactor promoter TOML for an HA profile")]
    async fn get_ha_profile_toml(
        &self,
        Parameters(p): Parameters<ProfileIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let Json(v) = ha::get_profile_toml(self.st(), Path(p.id_or_name))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    #[tool(
        description = "Create a full HA profile: optional LVM volume + DRBD resource + drbd-reactor promoter config + data migration. See the create-ha-service prompt for the workflow and field semantics."
    )]
    async fn create_ha_profile(
        &self,
        Parameters(req): Parameters<CreateHaProfileRequest>,
    ) -> Result<CallToolResult, McpError> {
        let (_status, Json(v)) = ha::create_profile(self.st(), Json(req))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    #[tool(
        description = "Activate an HA profile (deploy config to all nodes and start management)"
    )]
    async fn activate_ha_profile(
        &self,
        Parameters(p): Parameters<ProfileIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let Json(v) = ha::activate_profile(self.st(), Path(p.id_or_name))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    #[tool(
        description = "Deactivate an HA profile (stop managing; services are stopped). Disruptive — confirm with the user."
    )]
    async fn deactivate_ha_profile(
        &self,
        Parameters(p): Parameters<ProfileIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let Json(v) = ha::deactivate_profile(self.st(), Path(p.id_or_name))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    #[tool(description = "Re-enable a masked/disabled HA profile on all nodes")]
    async fn enable_ha_profile(
        &self,
        Parameters(p): Parameters<ProfileIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let Json(v) = ha::enable_profile(self.st(), Path(p.id_or_name))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    #[tool(
        description = "Evict an HA profile from a node (controlled failover to a peer). Disruptive — confirm before evicting production services."
    )]
    async fn evict_ha_profile(
        &self,
        Parameters(p): Parameters<EvictParams>,
    ) -> Result<CallToolResult, McpError> {
        let req = ha::EvictProfileRequest {
            node: p.node,
            delay: p.delay.unwrap_or(20),
            keep_masked: p.keep_masked,
            force: p.force,
        };
        let Json(v) = ha::evict_profile(self.st(), Path(p.id_or_name), Json(req))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    #[tool(
        description = "Delete an HA profile. With delete_resource=true also destroys the DRBD resource (DESTRUCTIVE — confirm with the user)."
    )]
    async fn delete_ha_profile(
        &self,
        Parameters(p): Parameters<DeleteProfileParams>,
    ) -> Result<CallToolResult, McpError> {
        let query = ha::DeleteProfileQuery {
            delete_resource: p.delete_resource,
            delete_config_file: p.delete_config_file,
        };
        let status = ha::delete_profile(self.st(), Path(p.id_or_name), Query(query))
            .await
            .map_err(app_err)?;
        ok_json(serde_json::json!({
            "deleted": true,
            "http_status": status.as_u16()
        }))
    }

    // -- drbd-reactor --

    #[tool(
        description = "drbd-reactor status across all nodes (per-profile enabled flag; missing enabled means true)"
    )]
    async fn reactor_status(&self) -> Result<CallToolResult, McpError> {
        let Json(v) = ha::reactor_status(self.st()).await.map_err(app_err)?;
        ok_json(v)
    }

    #[tool(description = "Reload or restart the drbd-reactor daemon on all nodes")]
    async fn reactor_reload(
        &self,
        Parameters(p): Parameters<ReactorReloadParams>,
    ) -> Result<CallToolResult, McpError> {
        let req = ha::ReactorReloadRequest {
            action: p.action.unwrap_or_else(|| "reload".to_string()),
        };
        let Json(v) = ha::reload_reactor(self.st(), Json(req))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }

    #[tool(description = "Get drbd-reactor journal logs")]
    async fn reactor_logs(
        &self,
        Parameters(p): Parameters<ReactorLogsParams>,
    ) -> Result<CallToolResult, McpError> {
        let query = ha::ReactorLogsQuery {
            lines: p.lines.unwrap_or(100),
            since: p.since,
        };
        let Json(v) = ha::reactor_logs(Query(query)).await.map_err(app_err)?;
        ok_json(v)
    }

    // -- Agents / services --

    #[tool(description = "List installed OCF resource agents (for VIP, Filesystem, etc.)")]
    async fn list_resource_agents(&self) -> Result<CallToolResult, McpError> {
        let Json(v) = ha::list_resource_agents().await.map_err(app_err)?;
        ok_json(v)
    }

    #[tool(description = "List systemd service unit files available for HA management")]
    async fn list_available_services(
        &self,
        Parameters(p): Parameters<ListServicesParams>,
    ) -> Result<CallToolResult, McpError> {
        let query = ha::ListServicesQuery {
            include_system: p.include_system,
        };
        let Json(v) = ha::list_available_services(Query(query))
            .await
            .map_err(app_err)?;
        ok_json(v)
    }
}

// ---- Prompts (skills) -----------------------------------------------------

const SKILL_MAIN: &str = include_str!("../../../skills/drbd-ha-ops/SKILL.md");
const SKILL_CREATE: &str =
    include_str!("../../../skills/drbd-ha-ops/references/create-ha-service.md");
const SKILL_FAILOVER: &str =
    include_str!("../../../skills/drbd-ha-ops/references/failover-and-recovery.md");
const SKILL_TROUBLESHOOT: &str =
    include_str!("../../../skills/drbd-ha-ops/references/troubleshooting.md");

fn skill_prompt(content: &str) -> Vec<PromptMessage> {
    vec![PromptMessage::new_text(PromptMessageRole::User, content)]
}

#[prompt_router]
impl DrbdHaMcp {
    #[prompt(
        name = "drbd-ha-ops",
        description = "Operating guide for this DRBD-HA cluster: tool catalog, safety rules, workflow index. Load this first."
    )]
    async fn drbd_ha_ops(&self) -> Vec<PromptMessage> {
        skill_prompt(SKILL_MAIN)
    }

    #[prompt(
        name = "create-ha-service",
        description = "Workflow: create a new HA service end-to-end (storage, DRBD resource, promoter config, verification)"
    )]
    async fn create_ha_service(&self) -> Vec<PromptMessage> {
        skill_prompt(SKILL_CREATE)
    }

    #[prompt(
        name = "failover-and-recovery",
        description = "Workflow: controlled failover (evict), moving services, recovering failed nodes"
    )]
    async fn failover_and_recovery(&self) -> Vec<PromptMessage> {
        skill_prompt(SKILL_FAILOVER)
    }

    #[prompt(
        name = "troubleshooting",
        description = "Workflow: diagnose degraded profiles, DRBD state cheat sheet, split-brain recovery"
    )]
    async fn troubleshooting(&self) -> Vec<PromptMessage> {
        skill_prompt(SKILL_TROUBLESHOOT)
    }
}

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for DrbdHaMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_instructions(
            "DRBD-HA cluster management. Load the 'drbd-ha-ops' prompt for the operating \
             guide and safety rules before mutating anything. Read-only tools (list_*, \
             get_*, *_status, *_logs, health, dashboard_summary) are always safe; \
             mutating tools (create/activate/deactivate/evict/delete/resource_action/\
             reactor_reload) change cluster state — verify with status tools afterwards.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evict_params_minimal_defaults_are_safe() {
        let p: EvictParams = serde_json::from_value(serde_json::json!({
            "id_or_name": "hamysql"
        }))
        .unwrap();
        assert_eq!(p.id_or_name, "hamysql");
        assert_eq!(p.node, None);
        assert_eq!(p.delay, None); // handler applies the 20s default
        assert!(!p.keep_masked);
        assert!(!p.force);
    }

    #[test]
    fn evict_params_full_roundtrip() {
        let p: EvictParams = serde_json::from_value(serde_json::json!({
            "id_or_name": "hamysql",
            "node": "orange3",
            "delay": 30,
            "keep_masked": true,
            "force": true
        }))
        .unwrap();
        assert_eq!(p.node.as_deref(), Some("orange3"));
        assert_eq!(p.delay, Some(30));
        assert!(p.keep_masked);
        assert!(p.force);
    }

    #[test]
    fn destructive_flags_default_to_non_destructive() {
        // A delete request that names only the profile must NOT also wipe the
        // DRBD resource or config file unless explicitly asked.
        let d: DeleteProfileParams = serde_json::from_value(serde_json::json!({
            "id_or_name": "hamysql"
        }))
        .unwrap();
        assert!(!d.delete_resource);
        assert!(!d.delete_config_file);

        let a: ResourceActionParams = serde_json::from_value(serde_json::json!({
            "name": "hamysql",
            "action": "down"
        }))
        .unwrap();
        assert!(!a.force);
    }

    #[test]
    fn required_field_is_enforced() {
        // Missing the required id_or_name must fail to deserialize.
        let r: Result<ProfileIdParam, _> = serde_json::from_value(serde_json::json!({}));
        assert!(r.is_err());
    }

    #[test]
    fn params_expose_json_schema() {
        // The MCP layer relies on these deriving a JSON Schema for tool specs.
        let schema = schemars::schema_for!(EvictParams);
        let json = serde_json::to_value(&schema).unwrap();
        let props = json.get("properties").expect("schema has properties");
        assert!(props.get("id_or_name").is_some());
        assert!(props.get("keep_masked").is_some());
    }
}

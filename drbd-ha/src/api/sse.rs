//! Server-Sent Events (SSE) for real-time updates
//!
//! Provides SSE endpoints for:
//! - DRBD resource status changes
//! - Node status updates
//! - Operation progress
//! - System events

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::Stream;
use serde::Serialize;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::broadcast;

use crate::core::{
    drbd_cmd::{parse_drbd_status, DrbdCmd, ResourceStatus},
    run_shell_command,
};
use crate::models::NodeStatus;
use crate::state::{AppState, BroadcastEvent, NotificationLevel};

/// SSE event types
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum SseEvent {
    /// DRBD resource status update
    #[serde(rename = "resource_status")]
    ResourceStatus(Vec<ResourceStatusEvent>),

    /// Single resource status change
    #[serde(rename = "resource_change")]
    ResourceChange(ResourceChangeEvent),

    /// Node status update
    #[serde(rename = "node_status")]
    NodeStatus(NodeStatusEvent),

    /// Operation progress update
    #[serde(rename = "progress")]
    Progress(ProgressEvent),

    /// System event (info, warning, error)
    #[serde(rename = "system")]
    System(SystemEvent),

    /// Heartbeat to keep connection alive
    #[serde(rename = "heartbeat")]
    Heartbeat { timestamp: i64 },
}

/// Resource status event data
#[derive(Debug, Clone, Serialize)]
pub struct ResourceStatusEvent {
    pub name: String,
    pub role: String,
    pub disk_state: String,
    pub connection_state: Option<String>,
    pub sync_percent: Option<f64>,
}

impl From<&ResourceStatus> for ResourceStatusEvent {
    fn from(status: &ResourceStatus) -> Self {
        let disk_state = status
            .devices
            .first()
            .map(|d| d.disk_state.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        let connection_state = status
            .connections
            .first()
            .map(|c| c.connection_state.clone());

        let sync_percent = status.sync_progress();

        Self {
            name: status.name.clone(),
            role: status.role.clone(),
            disk_state,
            connection_state,
            sync_percent,
        }
    }
}

/// Resource change event (when state changes)
#[derive(Debug, Clone, Serialize)]
pub struct ResourceChangeEvent {
    pub name: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
    pub timestamp: i64,
}

/// Node status event data
#[derive(Debug, Clone, Serialize)]
pub struct NodeStatusEvent {
    pub id: String,
    pub hostname: String,
    pub status: NodeStatus,
    pub last_seen: Option<i64>,
}

/// Operation progress event
#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub operation_id: String,
    pub operation: String,
    pub resource: Option<String>,
    pub progress: u8,
    pub message: String,
    pub completed: bool,
    pub success: Option<bool>,
}

/// System event (for notifications)
#[derive(Debug, Clone, Serialize)]
pub struct SystemEvent {
    pub level: EventLevel,
    pub message: String,
    pub source: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EventLevel {
    Info,
    Warning,
    Error,
}

/// GET /api/v1/events/resources
/// SSE stream for DRBD resource status updates
pub async fn resource_status_stream(
    State(_state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        let mut interval = tokio::time::interval(Duration::from_secs(2));

        loop {
            interval.tick().await;

            // Get DRBD resource status
            let status_cmd = crate::core::drbd_cmd::DrbdCmd::status_cmd();
            if let Ok(output) = crate::core::run_shell_command(&status_cmd, "Get DRBD status").await {
                if output.success() {
                    // Parse DRBD status and get sync progress
                    if let Ok(statuses) = crate::core::drbd_cmd::parse_drbd_status(&output.stdout) {
                        if !statuses.is_empty() {
                            // Convert to ResourceStatusEvent for SSE compatibility
                            let current: Vec<ResourceStatusEvent> = statuses.iter().map(|s| s.into()).collect();
                            if let Ok(json) = serde_json::to_string(&current) {
                                yield Ok(Event::default().event("resource_status").data(json));

                                // Send sync progress events for any resources that are syncing
                                for status_event in &current {
                                    if let Some(sync_percent) = status_event.sync_percent {
                                        let operation_id = format!("drbd_sync_{}", status_event.name);
                                        let progress = if sync_percent < 100.0 {
                                            Some(sync_percent as u8)
                                        } else {
                                            None
                                        };

                                        let progress_event = ProgressEvent {
                                            operation_id,
                                            operation: "drbd_sync".to_string(),
                                            resource: Some(status_event.name.clone()),
                                            progress: progress.unwrap_or(100),
                                            message: format!("DRBD resource '{}' is {}% synced", status_event.name, sync_percent),
                                            completed: sync_percent >= 100.0,
                                            success: Some(sync_percent >= 100.0),
                                        };

                                        if let Ok(progress_json) = serde_json::to_string(&progress_event) {
                                            yield Ok(Event::default().event("progress").data(progress_json));
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        yield Ok(Event::default().event("resource_status").data("[]"));
                    }
                } else {
                    yield Ok(Event::default().event("resource_status").data("[]"));
                }
            } else {
                yield Ok(Event::default().event("resource_status").data("[]"));
            }

            // Send heartbeat
            let heartbeat = SystemEvent {
                level: EventLevel::Info,
                message: "heartbeat".to_string(),
                source: "resource_status".to_string(),
                timestamp: chrono::Utc::now().timestamp(),
            };
            if let Ok(json) = serde_json::to_string(&heartbeat) {
                yield Ok(Event::default().event("heartbeat").data(json));
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    )
}

/// GET /api/v1/events/nodes
/// SSE stream for node status updates
pub async fn node_status_stream(
    State(_state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        let mut interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            interval.tick().await;

            // Disabled auto-polling to reduce log noise - only send heartbeat
            yield Ok(Event::default().event("heartbeat").data("{}"));
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    )
}

/// GET /api/v1/events/progress
/// SSE stream for operation progress (from broadcast channel)
pub async fn progress_stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.subscribe_events();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    match event {
                        BroadcastEvent::Progress {
                            operation_id,
                            operation,
                            resource,
                            progress,
                            message,
                            completed,
                            success,
                        } => {
                            let progress_event = ProgressEvent {
                                operation_id,
                                operation,
                                resource,
                                progress,
                                message,
                                completed,
                                success,
                            };
                            if let Ok(json) = serde_json::to_string(&progress_event) {
                                yield Ok(Event::default().event("progress").data(json));
                            }
                        }
                        BroadcastEvent::Notification { level, title, message } => {
                            let system_event = SystemEvent {
                                level: match level {
                                    NotificationLevel::Info => EventLevel::Info,
                                    NotificationLevel::Warning => EventLevel::Warning,
                                    NotificationLevel::Error => EventLevel::Error,
                                    NotificationLevel::Success => EventLevel::Info,
                                },
                                message: format!("{}: {}", title, message),
                                source: "system".to_string(),
                                timestamp: chrono::Utc::now().timestamp(),
                            };
                            if let Ok(json) = serde_json::to_string(&system_event) {
                                yield Ok(Event::default().event("notification").data(json));
                            }
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_n)) => {
                    // Client fell behind, just continue
                    // tracing::warn!("SSE client lagged by {} events", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    // Channel closed, end stream
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    )
}

/// GET /api/v1/events/all
/// Combined SSE stream for all events
pub async fn all_events_stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.subscribe_events();

    let stream = async_stream::stream! {
        let mut resource_interval = tokio::time::interval(Duration::from_secs(10));
        let mut node_interval = tokio::time::interval(Duration::from_secs(30));
        let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(30));

        let mut last_statuses: Vec<ResourceStatusEvent> = Vec::new();

        loop {
            tokio::select! {
                // Handle broadcast events (progress, notifications)
                result = rx.recv() => {
                    match result {
                        Ok(BroadcastEvent::Progress {
                            operation_id,
                            operation,
                            resource,
                            progress,
                            message,
                            completed,
                            success,
                        }) => {
                            let progress_event = ProgressEvent {
                                operation_id,
                                operation,
                                resource,
                                progress,
                                message,
                                completed,
                                success,
                            };
                            if let Ok(json) = serde_json::to_string(&progress_event) {
                                yield Ok(Event::default().event("progress").data(json));
                            }
                        }
                        Ok(BroadcastEvent::Notification { level, title, message }) => {
                            let system_event = SystemEvent {
                                level: match level {
                                    NotificationLevel::Info => EventLevel::Info,
                                    NotificationLevel::Warning => EventLevel::Warning,
                                    NotificationLevel::Error => EventLevel::Error,
                                    NotificationLevel::Success => EventLevel::Info,
                                },
                                message: format!("{}: {}", title, message),
                                source: "system".to_string(),
                                timestamp: chrono::Utc::now().timestamp(),
                            };
                            if let Ok(json) = serde_json::to_string(&system_event) {
                                yield Ok(Event::default().event("notification").data(json));
                            }
                        }
                        Err(_) => {}
                    }
                }
                _ = resource_interval.tick() => {
                    if let Ok(statuses) = get_drbd_status().await {
                        let current: Vec<ResourceStatusEvent> = statuses.iter().map(|s| s.into()).collect();

                        // Detect and send changes
                        let changes = detect_changes(&last_statuses, &current);
                        for change in changes {
                            let event = SseEvent::ResourceChange(change);
                            if let Ok(json) = serde_json::to_string(&event) {
                                yield Ok(Event::default().event("resource_change").data(json));
                            }
                        }

                        // Send status update
                        if let Ok(json) = serde_json::to_string(&current) {
                            yield Ok(Event::default().event("resource_status").data(json));
                        }

                        last_statuses = current;
                    }
                }
                _ = node_interval.tick() => {
                    if let Ok(nodes) = state.node_store.get_all() {
                        let events: Vec<NodeStatusEvent> = nodes
                            .iter()
                            .map(|n| NodeStatusEvent {
                                id: n.id.clone(),
                                hostname: n.hostname.clone(),
                                status: n.status.clone(),
                                last_seen: n.last_seen.map(|dt| dt.timestamp()),
                            })
                            .collect();

                        if let Ok(json) = serde_json::to_string(&events) {
                            yield Ok(Event::default().event("node_status").data(json));
                        }
                    }
                }
                _ = heartbeat_interval.tick() => {
                    let event = SseEvent::Heartbeat {
                        timestamp: chrono::Utc::now().timestamp(),
                    };
                    if let Ok(json) = serde_json::to_string(&event) {
                        yield Ok(Event::default().event("heartbeat").data(json));
                    }
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    )
}

/// Helper to get DRBD status (called frequently by SSE, minimal logging)
async fn get_drbd_status() -> Result<Vec<ResourceStatus>, String> {
    let cmd = DrbdCmd::status_cmd();
    // Use empty description to avoid log spam from periodic checks
    let output = run_shell_command(&cmd, "")
        .await
        .map_err(|e| e.to_string())?;

    if output.success() {
        parse_drbd_status(&output.stdout).map_err(|e| e.to_string())
    } else {
        Ok(Vec::new())
    }
}

/// Detect changes between old and new status
fn detect_changes(
    old: &[ResourceStatusEvent],
    new: &[ResourceStatusEvent],
) -> Vec<ResourceChangeEvent> {
    let mut changes = Vec::new();
    let timestamp = chrono::Utc::now().timestamp();

    for new_status in new {
        if let Some(old_status) = old.iter().find(|o| o.name == new_status.name) {
            // Check role change
            if old_status.role != new_status.role {
                changes.push(ResourceChangeEvent {
                    name: new_status.name.clone(),
                    field: "role".to_string(),
                    old_value: old_status.role.clone(),
                    new_value: new_status.role.clone(),
                    timestamp,
                });
            }

            // Check disk state change
            if old_status.disk_state != new_status.disk_state {
                changes.push(ResourceChangeEvent {
                    name: new_status.name.clone(),
                    field: "disk_state".to_string(),
                    old_value: old_status.disk_state.clone(),
                    new_value: new_status.disk_state.clone(),
                    timestamp,
                });
            }

            // Check connection state change
            if old_status.connection_state != new_status.connection_state {
                changes.push(ResourceChangeEvent {
                    name: new_status.name.clone(),
                    field: "connection_state".to_string(),
                    old_value: old_status.connection_state.clone().unwrap_or_default(),
                    new_value: new_status.connection_state.clone().unwrap_or_default(),
                    timestamp,
                });
            }
        } else {
            // New resource appeared
            changes.push(ResourceChangeEvent {
                name: new_status.name.clone(),
                field: "created".to_string(),
                old_value: String::new(),
                new_value: "new".to_string(),
                timestamp,
            });
        }
    }

    // Check for removed resources
    for old_status in old {
        if !new.iter().any(|n| n.name == old_status.name) {
            changes.push(ResourceChangeEvent {
                name: old_status.name.clone(),
                field: "deleted".to_string(),
                old_value: "existed".to_string(),
                new_value: String::new(),
                timestamp,
            });
        }
    }

    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_changes_role() {
        let old = vec![ResourceStatusEvent {
            name: "r0".to_string(),
            role: "Secondary".to_string(),
            disk_state: "UpToDate".to_string(),
            connection_state: Some("Connected".to_string()),
            sync_percent: None,
        }];

        let new = vec![ResourceStatusEvent {
            name: "r0".to_string(),
            role: "Primary".to_string(),
            disk_state: "UpToDate".to_string(),
            connection_state: Some("Connected".to_string()),
            sync_percent: None,
        }];

        let changes = detect_changes(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "role");
        assert_eq!(changes[0].old_value, "Secondary");
        assert_eq!(changes[0].new_value, "Primary");
    }

    #[test]
    fn test_detect_changes_new_resource() {
        let old: Vec<ResourceStatusEvent> = vec![];
        let new = vec![ResourceStatusEvent {
            name: "r0".to_string(),
            role: "Primary".to_string(),
            disk_state: "UpToDate".to_string(),
            connection_state: None,
            sync_percent: None,
        }];

        let changes = detect_changes(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "created");
    }

    #[test]
    fn test_detect_changes_deleted_resource() {
        let old = vec![ResourceStatusEvent {
            name: "r0".to_string(),
            role: "Primary".to_string(),
            disk_state: "UpToDate".to_string(),
            connection_state: None,
            sync_percent: None,
        }];
        let new: Vec<ResourceStatusEvent> = vec![];

        let changes = detect_changes(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "deleted");
    }

    #[test]
    fn test_sse_event_serialization() {
        let event = SseEvent::Heartbeat {
            timestamp: 1234567890,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("heartbeat"));
        assert!(json.contains("1234567890"));
    }
}

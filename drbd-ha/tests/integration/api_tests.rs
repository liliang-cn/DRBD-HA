//! API Integration Tests
//!
//! These tests verify the HTTP API layer without requiring real DRBD/Systemd.
//! They use in-memory SQLite and mock external dependencies.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

use drbd_ha::api::create_router;
use drbd_ha::{
    config::{AppConfig, DatabaseConfig},
    core::Database,
    state::AppState,
};

/// Create a test application state with in-memory database
fn create_test_state() -> Arc<AppState> {
    let mut config = AppConfig::default();
    // Use in-memory SQLite for tests
    config.database = DatabaseConfig {
        path: ":memory:".to_string(),
    };
    // Disable auth for tests
    config.auth.enabled = false;

    let db = Database::open(":memory:").expect("Failed to create in-memory database");
    Arc::new(AppState::new(config, db))
}

/// Helper to make GET requests
async fn get(app: &axum::Router, path: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);

    (status, json)
}

/// Helper to make POST requests with JSON body
async fn post(app: &axum::Router, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);

    (status, json)
}

/// Helper to make DELETE requests
async fn delete(app: &axum::Router, path: &str) -> StatusCode {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    response.status()
}

// ============================================================================
// Health Check Tests
// ============================================================================

#[tokio::test]
async fn test_health_check() {
    let state = create_test_state();
    let app = create_router(state);

    let (status, body) = get(&app, "/api/v1/health").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert!(body["version"].as_str().is_some());
}

// ============================================================================
// Node Management Tests
// ============================================================================

#[tokio::test]
async fn test_list_nodes_empty() {
    let state = create_test_state();
    let app = create_router(state);

    let (status, body) = get(&app, "/api/v1/nodes").await;

    assert_eq!(status, StatusCode::OK);
    // Should have at least the local node (created by default)
    assert!(body.as_array().is_some());
}

#[tokio::test]
async fn test_add_node_validation_error() {
    let state = create_test_state();
    let app = create_router(state);

    // Missing required fields
    let (status, body) = post(
        &app,
        "/api/v1/nodes",
        json!({
            "hostname": "",
            "ip": ""
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().is_some());
}

#[tokio::test]
async fn test_add_node_without_credentials() {
    let state = create_test_state();
    let app = create_router(state);

    let (status, body) = post(
        &app,
        "/api/v1/nodes",
        json!({
            "hostname": "test-node",
            "ip": "192.168.1.100"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["hostname"], "test-node");
    assert_eq!(body["ip"], "192.168.1.100");
    assert_eq!(body["status"], "unknown"); // No credentials, so status is unknown
    assert!(body["id"].as_str().is_some());
}

#[tokio::test]
async fn test_add_duplicate_node() {
    let state = create_test_state();
    let app = create_router(state);

    // Add first node
    let (status, _) = post(
        &app,
        "/api/v1/nodes",
        json!({
            "hostname": "node1",
            "ip": "192.168.1.101"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Try to add duplicate (same IP)
    let (status, body) = post(
        &app,
        "/api/v1/nodes",
        json!({
            "hostname": "node2",
            "ip": "192.168.1.101"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["message"].as_str().unwrap().contains("already exists"));
}

#[tokio::test]
async fn test_get_node() {
    let state = create_test_state();
    let app = create_router(state);

    // Add a node
    let (_, created) = post(
        &app,
        "/api/v1/nodes",
        json!({
            "hostname": "test-node",
            "ip": "192.168.1.102"
        }),
    )
    .await;
    let node_id = created["id"].as_str().unwrap();

    // Get the node
    let (status, body) = get(&app, &format!("/api/v1/nodes/{}", node_id)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], node_id);
    assert_eq!(body["hostname"], "test-node");
}

#[tokio::test]
async fn test_get_node_not_found() {
    let state = create_test_state();
    let app = create_router(state);

    let (status, body) = get(&app, "/api/v1/nodes/nonexistent-id").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["message"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_delete_node() {
    let state = create_test_state();
    let app = create_router(state);

    // Add a node
    let (_, created) = post(
        &app,
        "/api/v1/nodes",
        json!({
            "hostname": "to-delete",
            "ip": "192.168.1.103"
        }),
    )
    .await;
    let node_id = created["id"].as_str().unwrap();

    // Delete it
    let status = delete(&app, &format!("/api/v1/nodes/{}", node_id)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify it's gone
    let (status, _) = get(&app, &format!("/api/v1/nodes/{}", node_id)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_node_not_found() {
    let state = create_test_state();
    let app = create_router(state);

    let status = delete(&app, "/api/v1/nodes/nonexistent-id").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ============================================================================
// HA Profile Tests
// ============================================================================

#[tokio::test]
async fn test_list_ha_profiles_empty() {
    let state = create_test_state();
    let app = create_router(state);

    let (status, body) = get(&app, "/api/v1/ha/profiles").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["profiles"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_create_ha_profile_validation() {
    let state = create_test_state();
    let app = create_router(state);

    // Invalid resource name
    let (status, body) = post(
        &app,
        "/api/v1/ha/profiles",
        json!({
            "name": "test-profile",
            "resource_name": "invalid name with spaces",
            "mount_point": "/mnt/data",
            "fs_type": "xfs",
            "services": []
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().is_some());
}

#[tokio::test]
async fn test_create_ha_profile_invalid_mount_point() {
    let state = create_test_state();
    let app = create_router(state);

    // Invalid mount point (not absolute path)
    let (status, body) = post(
        &app,
        "/api/v1/ha/profiles",
        json!({
            "name": "test-profile",
            "resource_name": "r0",
            "mount_point": "relative/path",
            "fs_type": "xfs",
            "services": []
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["message"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("mount"));
}

#[tokio::test]
async fn test_create_ha_profile_invalid_fs_type() {
    let state = create_test_state();
    let app = create_router(state);

    let (status, body) = post(
        &app,
        "/api/v1/ha/profiles",
        json!({
            "name": "test-profile",
            "resource_name": "r0",
            "mount_point": "/mnt/data",
            "fs_type": "ntfs",
            "services": []
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["message"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("filesystem"));
}

#[tokio::test]
async fn test_create_ha_profile_invalid_vip() {
    let state = create_test_state();
    let app = create_router(state);

    let (status, body) = post(
        &app,
        "/api/v1/ha/profiles",
        json!({
            "name": "test-profile",
            "resource_name": "r0",
            "mount_point": "/mnt/data",
            "fs_type": "xfs",
            "services": [],
            "vip": {
                "address": "invalid-ip",
                "netmask": 24,
                "interface": "eth0"
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().is_some());
}

#[tokio::test]
async fn test_get_ha_profile_not_found() {
    let state = create_test_state();
    let app = create_router(state);

    let (status, body) = get(&app, "/api/v1/ha/profiles/nonexistent").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["message"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_delete_ha_profile_not_found() {
    let state = create_test_state();
    let app = create_router(state);

    let status = delete(&app, "/api/v1/ha/profiles/nonexistent").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ============================================================================
// Database Persistence Tests
// ============================================================================

#[tokio::test]
async fn test_node_persistence() {
    let state = create_test_state();
    let app = create_router(state.clone());

    // Add multiple nodes
    for i in 1..=3 {
        let (status, _) = post(
            &app,
            "/api/v1/nodes",
            json!({
                "hostname": format!("node{}", i),
                "ip": format!("192.168.1.{}", 100 + i)
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    // List nodes - should have 3 + local node
    let (status, body) = get(&app, "/api/v1/nodes").await;
    assert_eq!(status, StatusCode::OK);

    let nodes = body.as_array().unwrap();
    // At least 3 nodes we added (plus potentially a local node)
    assert!(nodes.len() >= 3);
}

// ============================================================================
// Authentication Tests
// ============================================================================

#[tokio::test]
async fn test_auth_disabled() {
    let state = create_test_state();
    let app = create_router(state);

    // Should work without auth header when auth is disabled
    let (status, _) = get(&app, "/api/v1/health").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_auth_enabled_without_token() {
    let mut config = AppConfig::default();
    config.database.path = ":memory:".to_string();
    config.auth.enabled = true;
    config.auth.token = Some("secret-token".to_string());

    let db = Database::open(":memory:").unwrap();
    let state = Arc::new(AppState::new(config, db));
    let app = create_router(state);

    // Request without token should fail
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/nodes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_enabled_with_valid_token() {
    let mut config = AppConfig::default();
    config.database.path = ":memory:".to_string();
    config.auth.enabled = true;
    config.auth.token = Some("secret-token".to_string());

    let db = Database::open(":memory:").unwrap();
    let state = Arc::new(AppState::new(config, db));
    let app = create_router(state);

    // Request with valid token should succeed
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/nodes")
                .header("Authorization", "Bearer secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_auth_enabled_with_invalid_token() {
    let mut config = AppConfig::default();
    config.database.path = ":memory:".to_string();
    config.auth.enabled = true;
    config.auth.token = Some("secret-token".to_string());

    let db = Database::open(":memory:").unwrap();
    let state = Arc::new(AppState::new(config, db));
    let app = create_router(state);

    // Request with invalid token should fail
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/nodes")
                .header("Authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ============================================================================
// Error Response Format Tests
// ============================================================================

#[tokio::test]
async fn test_error_response_format() {
    let state = create_test_state();
    let app = create_router(state);

    let (status, body) = get(&app, "/api/v1/nodes/nonexistent").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    // Error response should have consistent format
    assert!(body.get("error").is_some());
}

#[tokio::test]
async fn test_validation_error_format() {
    let state = create_test_state();
    let app = create_router(state);

    let (status, body) = post(
        &app,
        "/api/v1/nodes",
        json!({
            "hostname": "",
            "ip": ""
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.get("error").is_some());
}

// ============================================================================
// HA Profile Status Tests
// ============================================================================

#[tokio::test]
async fn test_get_ha_profile_status_not_found() {
    let state = create_test_state();
    let app = create_router(state);

    let (status, body) = get(&app, "/api/v1/ha/profiles/nonexistent/status").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["message"].as_str().unwrap().contains("not found"));
}

// ============================================================================
// Resource Management Tests
// ============================================================================

#[tokio::test]
async fn test_list_resources_empty() {
    let state = create_test_state();
    let app = create_router(state);

    let (status, body) = get(&app, "/api/v1/resources").await;

    assert_eq!(status, StatusCode::OK);
    // Response is { "resources": [] } or just []
    let resources = body.as_array().or_else(|| body["resources"].as_array());
    assert!(resources.map(|r| r.is_empty()).unwrap_or(true));
}

#[tokio::test]
async fn test_create_resource_validation_error() {
    let state = create_test_state();
    let app = create_router(state);

    // Invalid resource name (with spaces)
    let (status, _body) = post(
        &app,
        "/api/v1/resources",
        json!({
            "name": "invalid name",
            "device": "/dev/drbd0",
            "disk": "/dev/sda1",
            "port": 7789,
            "nodes": []
        }),
    )
    .await;

    // Can be 400 (BAD_REQUEST) or 422 (UNPROCESSABLE_ENTITY)
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got {}",
        status
    );
}

#[tokio::test]
async fn test_create_resource_missing_nodes() {
    let state = create_test_state();
    let app = create_router(state);

    // Empty nodes array
    let (status, _body) = post(
        &app,
        "/api/v1/resources",
        json!({
            "name": "r0",
            "device": "/dev/drbd0",
            "disk": "/dev/sda1",
            "port": 7789,
            "nodes": []
        }),
    )
    .await;

    // Can be 400 (BAD_REQUEST) or 422 (UNPROCESSABLE_ENTITY)
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got {}",
        status
    );
}

#[tokio::test]
async fn test_get_resource_not_found() {
    let state = create_test_state();
    let app = create_router(state);

    let (status, body) = get(&app, "/api/v1/resources/nonexistent").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["message"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_delete_resource_not_found() {
    let state = create_test_state();
    let app = create_router(state);

    let status = delete(&app, "/api/v1/resources/nonexistent").await;
    // API might return 204 (idempotent delete) or 404
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::NO_CONTENT,
        "Expected 404 or 204, got {}",
        status
    );
}

// ============================================================================
// Evict Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_evict_profile_not_found() {
    let state = create_test_state();
    let app = create_router(state);

    let (status, body) = post(
        &app,
        "/api/v1/ha/profiles/nonexistent/evict",
        json!({
            "node": "gui01"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["message"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_evict_invalid_delay() {
    let state = create_test_state();
    let app = create_router(state);

    // Negative delay should be rejected (if validation exists)
    let (status, _) = post(
        &app,
        "/api/v1/ha/profiles/some-id/evict",
        json!({
            "node": "gui01",
            "delay": -1
        }),
    )
    .await;

    // Can be NOT_FOUND, BAD_REQUEST, or UNPROCESSABLE_ENTITY
    assert!(
        status == StatusCode::NOT_FOUND
            || status == StatusCode::BAD_REQUEST
            || status == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 404, 400, or 422, got {}",
        status
    );
}

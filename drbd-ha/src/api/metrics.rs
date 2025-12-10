//! Metrics API endpoints for Prometheus integration

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info};

use crate::{core::metrics::METRICS, error::AppError, state::AppState};

/// Get Prometheus metrics
///
/// This endpoint returns metrics in the Prometheus text format.
/// It can be scraped by a Prometheus server.
///
/// # Responses
///
/// * 200 - Metrics in Prometheus text format
/// * 500 - Internal server error
#[utoipa::path(
    get,
    path = "/metrics",
    tag = "Metrics",
    summary = "Get Prometheus metrics",
    description = "Returns metrics in Prometheus text format for scraping",
    responses(
        (status = 200, description = "Metrics in Prometheus format", content_type = "text/plain"),
        (status = 500, description = "Internal server error", body = serde_json::Value)
    )
)]
pub async fn get_metrics(State(_state): State<Arc<AppState>>) -> Result<Response, AppError> {
    info!("Metrics endpoint requested");

    match METRICS.export() {
        Ok(metrics_text) => Ok((
            StatusCode::OK,
            [("content-type", "text/plain; version=0.0.4")],
            metrics_text,
        )
            .into_response()),
        Err(e) => {
            error!("Failed to export metrics: {}", e);
            Err(AppError::Internal(format!(
                "Failed to export metrics: {}",
                e
            )))
        }
    }
}

/// Get metrics summary in JSON format
///
/// This endpoint returns a summary of key metrics in JSON format,
/// suitable for dashboard display or quick health checks.
///
/// # Responses
///
/// * 200 - Metrics summary in JSON format
/// * 500 - Internal server error
#[utoipa::path(
    get,
    path = "/api/v1/metrics/summary",
    tag = "Metrics",
    summary = "Get metrics summary",
    description = "Returns a summary of key metrics in JSON format",
    responses(
        (status = 200, description = "Metrics summary", body = serde_json::Value),
        (status = 500, description = "Internal server error", body = serde_json::Value)
    )
)]
pub async fn get_metrics_summary(
    State(_state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    info!("Metrics summary endpoint requested");

    // For now, return a basic summary. In a real implementation,
    // you might want to parse the metrics text and extract specific values.
    let summary = json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "system": {
            "nodes_total": "N/A",
            "nodes_online": "N/A",
            "resources_total": "N/A",
            "resources_healthy": "N/A",
            "resources_degraded": "N/A"
        },
        "drbd": {
            "connections": "N/A",
            "sync_bytes_total": "N/A"
        },
        "ha_profiles": {
            "total": "N/A",
            "active": "N/A",
            "standby": "N/A",
            "failed": "N/A"
        },
        "storage": {
            "pools_total": "N/A",
            "volumes_total": "N/A"
        },
        "api": {
            "requests_total": "N/A",
            "active_connections": "N/A"
        }
    });

    Ok((StatusCode::OK, axum::Json(summary)))
}

/// Health check with metrics
///
/// This endpoint combines health status with basic metrics information.
/// Useful for load balancers and monitoring systems.
///
/// # Responses
///
/// * 200 - Health status with metrics
/// * 503 - Service unhealthy
#[utoipa::path(
    get,
    path = "/api/v1/health/metrics",
    tag = "Metrics",
    summary = "Health check with metrics",
    description = "Returns health status along with basic metrics",
    responses(
        (status = 200, description = "Health status with metrics", body = serde_json::Value),
        (status = 503, description = "Service unhealthy", body = serde_json::Value)
    )
)]
pub async fn health_with_metrics(
    State(_state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    info!("Health with metrics endpoint requested");

    let health_status = json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
        "metrics_available": true,
        "uptime_seconds": "N/A", // Could be calculated from application start time
        "memory_usage": "N/A",   // Could be collected from system
        "cpu_usage": "N/A"       // Could be collected from system
    });

    Ok((StatusCode::OK, axum::Json(health_status)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, AuthConfig, DatabaseConfig};
    use crate::core::Database;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    fn create_test_state() -> Arc<AppState> {
        let config = AppConfig {
            database: DatabaseConfig {
                path: ":memory:".to_string(),
            },
            auth: AuthConfig {
                enabled: false,
                ..Default::default()
            },
            ..AppConfig::default()
        };

        let db = Database::open(":memory:").expect("Failed to create in-memory database");
        Arc::new(AppState::new(config, db))
    }

    #[tokio::test]
    async fn test_get_metrics() {
        let state = create_test_state();
        let app = axum::Router::new()
            .route("/metrics", axum::routing::get(get_metrics))
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Check content type
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/plain; version=0.0.4"
        );

        // Check that response contains metrics
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        // Should contain some basic metrics
        assert!(body_str.contains("drbd_ha_"));
    }

    #[tokio::test]
    async fn test_get_metrics_summary() {
        let state = create_test_state();
        let app = axum::Router::new()
            .route(
                "/api/v1/metrics/summary",
                axum::routing::get(get_metrics_summary),
            )
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/metrics/summary")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json["system"].is_object());
        assert!(json["drbd"].is_object());
        assert!(json["ha_profiles"].is_object());
        assert!(json["storage"].is_object());
        assert!(json["api"].is_object());
    }

    #[tokio::test]
    async fn test_health_with_metrics() {
        let state = create_test_state();
        let app = axum::Router::new()
            .route(
                "/api/v1/health/metrics",
                axum::routing::get(health_with_metrics),
            )
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/health/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], "ok");
        assert_eq!(json["metrics_available"], true);
        assert!(json["version"].is_string());
    }
}

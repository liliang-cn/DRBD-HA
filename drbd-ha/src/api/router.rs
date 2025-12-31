//! API router configuration

use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tower_http::compression::CompressionLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use super::{
    cluster, dashboard, doc::ApiDoc, ha, metrics, middleware::auth_middleware, resource, sse,
    storage, ui,
};
use crate::state::AppState;

/// Create the main API router
pub fn create_router(state: Arc<AppState>) -> Router {
    // Generate OpenAPI spec in a separate thread with a larger stack to avoid overflow
    // The ApiDoc struct is very large and can cause stack overflow on default threads
    let openapi = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024) // 16MB stack (increased from 8MB due to overflow)
        .spawn(ApiDoc::openapi)
        .expect("Failed to spawn thread for OpenAPI generation")
        .join()
        .expect("OpenAPI generation thread panicked");

    let api_routes = Router::new()
        // Health check
        .route("/health", get(cluster::health_check))
        .route("/health/metrics", get(metrics::health_with_metrics))
        // Metrics
        .route("/metrics/summary", get(metrics::get_metrics_summary))
        // Dashboard
        .route("/dashboard/summary", get(dashboard::get_summary))
        // Node management
        .route("/nodes", get(cluster::list_nodes))
        .route("/nodes", post(cluster::add_node))
        .route("/nodes/{id}", get(cluster::get_node))
        .route("/nodes/{id}", delete(cluster::delete_node))
        .route("/nodes/{id}/disks", get(cluster::list_node_disks))
        .route(
            "/nodes/{id}/disks/available",
            get(cluster::list_available_disks),
        )
        .route("/nodes/{id}/check", post(cluster::check_node_status))
        // DRBD Resource management
        .route("/resources", get(resource::list_resources))
        .route("/resources", post(resource::create_resource))
        .route("/resources/{name}", get(resource::get_resource))
        .route("/resources/{name}", delete(resource::delete_resource))
        .route("/resources/{name}/action", post(resource::resource_action))
        .route("/resources/{name}/init", post(resource::init_resource))
        .route("/resources/{name}/mkfs", post(resource::create_filesystem))
        .route("/resources/{name}/mount", post(resource::mount_resource))
        .route("/resources/{name}/umount", post(resource::umount_resource))
        .route("/resources/{name}/logs", get(resource::get_resource_logs))
        // Storage Pool management
        .route("/pools", get(storage::list_pools))
        .route("/pools", post(storage::create_pool))
        .route("/pools/{pool_id}/volumes", post(storage::create_volume))
        // Zpool check
        .route("/storage/zpool/check", get(storage::check_zpool))
        .route("/storage/zpool/check/{node_id}", get(storage::check_zpool_on_node))
        // HA Profile management
        .route("/ha/profiles", get(ha::list_profiles))
        .route("/ha/profiles", post(ha::create_profile))
        .route("/ha/profiles/{id}", get(ha::get_profile))
        .route("/ha/profiles/{id}", delete(ha::delete_profile))
        .route("/ha/profiles/{id}/status", get(ha::get_profile_status))
        .route("/ha/profiles/{id}/activate", post(ha::activate_profile))
        .route("/ha/profiles/{id}/deactivate", post(ha::deactivate_profile))
        .route("/ha/profiles/{id}/enable", post(ha::enable_profile))
        .route("/ha/profiles/{id}/evict", post(ha::evict_profile))
        .route("/ha/profiles/{id}/{node}/disable", post(ha::disable_profile_on_node))
        .route("/ha/profiles/{id}/{node}/enable", post(ha::enable_profile_on_node))
        .route("/ha/profiles/{id}/vip", post(ha::add_vip))
        .route("/ha/profiles/{id}/vip", delete(ha::remove_vip))
        .route("/ha/profiles/{id}/toml", get(ha::get_profile_toml))
        .route("/ha/profiles/{id}/toml", axum::routing::put(ha::update_profile_toml))
        .route("/ha/profiles/{id}/toml/sync", post(ha::sync_profile_toml))
        .route("/ha/profiles/{id}/toml/parse", get(ha::parse_profile_toml))
        .route("/ha/profiles/{id}/start-array", axum::routing::put(ha::update_start_array))
        // Discovery and Import
        .route("/ha/unmanaged", get(ha::list_unmanaged_profiles))
        .route("/ha/import", post(ha::import_profiles))
        // Resource Agent management
        .route("/ha/resource-agents", get(ha::list_resource_agents))
        .route("/ha/resource-agents/all", get(ha::list_all_resource_agents))
        .route(
            "/ha/resource-agents/{provider}/{agent}",
            get(ha::get_resource_agent_metadata),
        )
        // drbd-reactor management
        .route("/ha/reactor/status", get(ha::reactor_status))
        .route("/ha/reactor/reload", post(ha::reload_reactor))
        .route("/ha/reactor/logs", get(ha::reactor_logs))
        // Systemd service listing (for HA service selection)
        .route("/services", get(ha::list_services))
        .route("/services/available", get(ha::list_available_services))
        // SSE event streams
        .route("/events/resources", get(sse::resource_status_stream))
        .route("/events/nodes", get(sse::node_status_stream))
        .route("/events/progress", get(sse::progress_stream))
        .route("/events/all", get(sse::all_events_stream));

    Router::new()
        // Prometheus metrics endpoint (outside auth for easy scraping)
        .route("/metrics", get(metrics::get_metrics))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi))
        .nest("/api/v1", api_routes)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<axum::body::Body>| {
                    let path = request.uri().path();
                    // Use DEBUG level for SSE endpoints so they are hidden by default
                    if path.starts_with("/api/v1/events") {
                        tracing::debug_span!(
                            "http_request",
                            method = ?request.method(),
                            uri = ?request.uri(),
                        )
                    } else {
                        tracing::info_span!(
                            "http_request",
                            method = ?request.method(),
                            uri = ?request.uri(),
                        )
                    }
                })
                .on_response(
                    |response: &axum::http::Response<axum::body::Body>,
                     latency: std::time::Duration,
                     _span: &tracing::Span| {
                        // Log response details at the same level as the request span ideally,
                        // but since we can't easily detect the path here, we'll just use DEBUG
                        // for the response log to keep the console clean.
                        // Important info is already captured in the request span (method, uri).
                        tracing::debug!(
                            "response generated in {:?} with status {}",
                            latency,
                            response.status()
                        );
                    },
                ),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(CompressionLayer::new().gzip(true))
        .with_state(state)
        // Serve embedded UI - must be after API routes
        .route("/", get(ui::serve_index))
        .fallback(ui::serve_ui)
}

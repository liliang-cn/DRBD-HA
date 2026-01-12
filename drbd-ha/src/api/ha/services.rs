use axum::{extract::Query, Json};

use crate::core::systemd_ctrl::SystemdController;
use crate::error::AppResult;

use super::types::{ListServicesQuery, ServiceFileListResponse, ServiceListResponse};

/// GET /api/v1/services
#[utoipa::path(
    get,
    path = "/api/v1/services",
    tag = "ha",
    params(
        ("include_system" = Option<bool>, Query, description = "Include system services (default: false)")
    ),
    responses(
        (status = 200, description = "List of systemd services", body = ServiceListResponse)
    )
)]
pub async fn list_services(
    Query(query): Query<ListServicesQuery>,
) -> AppResult<Json<ServiceListResponse>> {
    let systemd = SystemdController::new().await?;
    let services = systemd.list_services(query.include_system).await?;
    Ok(Json(ServiceListResponse { services }))
}

/// GET /api/v1/services/available
#[utoipa::path(
    get,
    path = "/api/v1/services/available",
    tag = "ha",
    params(
        ("include_system" = Option<bool>, Query, description = "Include system services (default: false)")
    ),
    responses(
        (status = 200, description = "List of available systemd service files", body = ServiceFileListResponse)
    )
)]
pub async fn list_available_services(
    Query(query): Query<ListServicesQuery>,
) -> AppResult<Json<ServiceFileListResponse>> {
    let systemd = SystemdController::new().await?;
    let services = systemd.list_service_files(query.include_system).await?;
    Ok(Json(ServiceFileListResponse { services }))
}

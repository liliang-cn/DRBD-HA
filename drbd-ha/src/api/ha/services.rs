use axum::{extract::Query, Json};

use crate::core::systemd_ctrl::SystemdController;
use crate::error::AppResult;

use super::types::{ListServicesQuery, ServiceFileListResponse, ServiceListResponse};

/// GET /api/v1/services
pub async fn list_services(
    Query(query): Query<ListServicesQuery>,
) -> AppResult<Json<ServiceListResponse>> {
    let systemd = SystemdController::new().await?;
    let services = systemd.list_services(query.include_system).await?;
    Ok(Json(ServiceListResponse { services }))
}

/// GET /api/v1/services/available
pub async fn list_available_services(
    Query(query): Query<ListServicesQuery>,
) -> AppResult<Json<ServiceFileListResponse>> {
    let systemd = SystemdController::new().await?;
    let services = systemd.list_service_files(query.include_system).await?;
    Ok(Json(ServiceFileListResponse { services }))
}

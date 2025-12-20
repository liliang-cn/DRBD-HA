//! Wizard session API handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::error::{AppError, AppResult};
use crate::models::{WizardSession, WizardMode, WizardSessionRequest};
use crate::state::AppState;

/// GET /api/v1/wizard/sessions
#[utoipa::path(
    get,
    path = "/api/v1/wizard/sessions",
    tag = "wizard",
    params(
        ("mode" = Option<String>, Query, description = "Filter by mode (service or storage)"),
        ("limit" = Option<i32>, Query, description = "Limit number of sessions (default 10)")
    ),
    responses(
        (status = 200, description = "List of wizard sessions", body = [WizardSession])
    )
)]
pub async fn list_wizard_sessions(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> AppResult<Json<Vec<WizardSession>>> {
    let mode = params.get("mode")
        .and_then(|m| match m.as_str() {
            "service" => Some(WizardMode::Service),
            "storage" => Some(WizardMode::Storage),
            _ => None,
        })
        .unwrap_or(WizardMode::Service);

    let limit = params.get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(10);

    let sessions = state.db.get_recent_wizard_sessions(&mode, limit)?;
    Ok(Json(sessions))
}

/// POST /api/v1/wizard/sessions
#[utoipa::path(
    post,
    path = "/api/v1/wizard/sessions",
    tag = "wizard",
    request_body = WizardSessionRequest,
    responses(
        (status = 201, description = "Wizard session created", body = WizardSession),
        (status = 400, description = "Validation error")
    )
)]
pub async fn create_wizard_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<WizardSessionRequest>,
) -> AppResult<(StatusCode, Json<WizardSession>)> {
    let mut session = WizardSession::new(req.mode);
    session.current_step = req.current_step;
    session.step_data = req.step_data;

    state.db.insert_wizard_session(&session)?;

    Ok((StatusCode::CREATED, Json(session)))
}

/// GET /api/v1/wizard/sessions/:id
#[utoipa::path(
    get,
    path = "/api/v1/wizard/sessions/{id}",
    tag = "wizard",
    params(
        ("id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 200, description = "Wizard session details", body = WizardSession),
        (status = 404, description = "Session not found")
    )
)]
pub async fn get_wizard_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<WizardSession>> {
    state
        .db
        .get_wizard_session(&id)?
        .map(Json)
        .ok_or_else(|| AppError::NotFound(format!("Wizard session {} not found", id)))
}

/// PUT /api/v1/wizard/sessions/:id
#[utoipa::path(
    put,
    path = "/api/v1/wizard/sessions/{id}",
    tag = "wizard",
    params(
        ("id" = String, Path, description = "Session ID")
    ),
    request_body = WizardSessionRequest,
    responses(
        (status = 200, description = "Wizard session updated", body = WizardSession),
        (status = 404, description = "Session not found"),
        (status = 400, description = "Validation error")
    )
)]
pub async fn update_wizard_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<WizardSessionRequest>,
) -> AppResult<Json<WizardSession>> {
    let mut session = state
        .db
        .get_wizard_session(&id)?
        .ok_or_else(|| AppError::NotFound(format!("Wizard session {} not found", id)))?;

    // Update session data
    session.mode = req.mode;
    session.update_step(req.current_step, req.step_data);

    state.db.update_wizard_session(&session)?;
    Ok(Json(session))
}

/// DELETE /api/v1/wizard/sessions/:id
#[utoipa::path(
    delete,
    path = "/api/v1/wizard/sessions/{id}",
    tag = "wizard",
    params(
        ("id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 204, description = "Session deleted"),
        (status = 404, description = "Session not found")
    )
)]
pub async fn delete_wizard_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    if state.db.delete_wizard_session(&id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound(format!("Wizard session {} not found", id)))
    }
}

/// Save progress for a specific wizard step
/// POST /api/v1/wizard/sessions/:id/step/:step_number
#[utoipa::path(
    post,
    path = "/api/v1/wizard/sessions/{id}/step/{step_number}",
    tag = "wizard",
    params(
        ("id" = String, Path, description = "Session ID"),
        ("step_number" = u32, Path, description = "Step number (0-4)")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Step data saved", body = WizardSession),
        (status = 404, description = "Session not found"),
        (status = 400, description = "Invalid step number")
    )
)]
pub async fn save_wizard_step(
    State(state): State<Arc<AppState>>,
    Path((id, step_number)): Path<(String, u32)>,
    Json(step_data): Json<serde_json::Value>,
) -> AppResult<Json<WizardSession>> {
    if step_number > 4 {
        return Err(AppError::Validation("Invalid step number. Must be between 0 and 4".to_string()));
    }

    let mut session = state
        .db
        .get_wizard_session(&id)?
        .ok_or_else(|| AppError::NotFound(format!("Wizard session {} not found", id)))?;

    // Update step data
    session.update_step(step_number, step_data);

    state.db.update_wizard_session(&session)?;
    Ok(Json(session))
}
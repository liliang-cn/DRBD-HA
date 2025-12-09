//! API middleware for authentication and other concerns

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use crate::state::AppState;

/// Token authentication middleware
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip auth if not enabled
    if !state.config.auth.is_required() {
        return Ok(next.run(request).await);
    }

    // Skip auth for health check endpoint
    if request.uri().path() == "/api/v1/health" {
        return Ok(next.run(request).await);
    }

    // Get token from Authorization header
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    let token = match auth_header {
        Some(header) if header.starts_with("Bearer ") => &header[7..],
        Some(header) if header.starts_with("Token ") => &header[6..],
        _ => {
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Validate token
    if state.config.auth.validate_token(token) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

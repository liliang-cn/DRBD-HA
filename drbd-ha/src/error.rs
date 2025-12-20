//! Unified error handling for DRBD HA Manager
//!
//! This module provides a centralized error type that can be used throughout
//! the application and automatically converted to HTTP responses.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

/// Application-wide error type
#[derive(Error, Debug)]
pub enum AppError {
    /// SSH connection or command execution error
    #[error("SSH error: {0}")]
    Ssh(String),

    /// DRBD command execution error
    #[error("DRBD error: {0}")]
    Drbd(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Validation error (invalid input)
    #[error("Validation error: {0}")]
    Validation(String),

    /// Resource not found
    #[error("Resource not found: {0}")]
    NotFound(String),

    /// Resource already exists
    #[error("Resource already exists: {0}")]
    AlreadyExists(String),

    /// Operation conflict (e.g., resource busy)
    #[error("Operation conflict: {0}")]
    Conflict(String),

    /// Transaction error (operation failed, rollback performed)
    #[error("Transaction failed: {0}")]
    Transaction(String),

    /// Internal server error
    #[error("Internal server error: {0}")]
    Internal(String),

    /// Systemd D-Bus error
    #[error("Systemd error: {0}")]
    Systemd(String),

    /// Network connectivity error
    #[error("Network error: {0}")]
    Network(String),

    /// Database error
    #[error("Database error: {0}")]
    Database(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// TOML parsing error
    #[error("TOML error: {0}")]
    Toml(String),

    #[error("Migration error: {0}")]
    Migration(String),

    /// Operation timeout
    #[error("Operation timeout: {0}")]
    Timeout(String),
}

impl From<systemd_utils::SystemdError> for AppError {
    fn from(err: systemd_utils::SystemdError) -> Self {
        AppError::Systemd(err.to_string())
    }
}

impl From<drbd_migration::error::MigrationError> for AppError {
    fn from(err: drbd_migration::error::MigrationError) -> Self {
        AppError::Migration(err.to_string())
    }
}

use utoipa::ToSchema;

/// Error response body for API
#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_type, message) = match &self {
            AppError::Ssh(msg) => (StatusCode::BAD_GATEWAY, "ssh_error", msg.clone()),
            AppError::Drbd(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "drbd_error", msg.clone()),
            AppError::Config(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "config_error",
                msg.clone(),
            ),
            AppError::Validation(msg) => (StatusCode::BAD_REQUEST, "validation_error", msg.clone()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone()),
            AppError::AlreadyExists(msg) => (StatusCode::CONFLICT, "already_exists", msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg.clone()),
            AppError::Transaction(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "transaction_error",
                msg.clone(),
            ),
            AppError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                msg.clone(),
            ),
            AppError::Systemd(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "systemd_error",
                msg.clone(),
            ),
            AppError::Network(msg) => (StatusCode::BAD_GATEWAY, "network_error", msg.clone()),
            AppError::Database(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                msg.clone(),
            ),
            AppError::Io(e) => (StatusCode::INTERNAL_SERVER_ERROR, "io_error", e.to_string()),
            AppError::Json(e) => (StatusCode::BAD_REQUEST, "json_error", e.to_string()),
            AppError::Toml(msg) => (StatusCode::BAD_REQUEST, "toml_error", msg.clone()),
            AppError::Migration(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "migration_error",
                msg.clone(),
            ),
            AppError::Timeout(msg) => (
                StatusCode::REQUEST_TIMEOUT,
                "timeout_error",
                msg.clone(),
            ),
        };

        let body = ErrorResponse {
            error: error_type.to_string(),
            message,
            details: None,
        };

        (status, Json(body)).into_response()
    }
}

/// Result type alias using AppError
pub type AppResult<T> = Result<T, AppError>;

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<toml::de::Error> for AppError {
    fn from(err: toml::de::Error) -> Self {
        AppError::Toml(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = AppError::Validation("invalid resource name".to_string());
        assert_eq!(err.to_string(), "Validation error: invalid resource name");
    }

    #[test]
    fn test_error_response_serialization() {
        let response = ErrorResponse {
            error: "validation_error".to_string(),
            message: "invalid input".to_string(),
            details: Some("field 'name' is required".to_string()),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("validation_error"));
        assert!(json.contains("invalid input"));
    }
}

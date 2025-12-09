//! DRBD High Availability Manager
//!
//! A Rust-based management system for DRBD high availability clusters.
//! Provides REST API for managing DRBD resources, drbd-reactor integration,
//! and service HA orchestration.

pub mod api;
pub mod config;
pub mod core;
pub mod error;
pub mod models;
pub mod state;

pub use config::AppConfig;
pub use error::{AppError, AppResult};
pub use state::AppState;

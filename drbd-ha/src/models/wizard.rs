//! Wizard session models for step persistence

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Wizard session mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum WizardMode {
    Service,
    Storage,
}

/// Wizard session for tracking progress
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WizardSession {
    pub id: String,
    pub mode: WizardMode,
    pub current_step: u32,
    pub step_data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create or update a wizard session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WizardSessionRequest {
    pub mode: WizardMode,
    pub current_step: u32,
    pub step_data: serde_json::Value,
}

impl WizardSession {
    /// Create a new wizard session
    pub fn new(mode: WizardMode) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            mode,
            current_step: 0,
            step_data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }

    /// Update the session with new step data
    pub fn update_step(&mut self, step: u32, data: serde_json::Value) {
        self.current_step = step;
        // Merge step data with existing data
        match (&self.step_data, &data) {
            (serde_json::Value::Object(existing_map), serde_json::Value::Object(new_data)) => {
                let mut combined_map = existing_map.clone();
                for (key, value) in new_data {
                    combined_map.insert(key.clone(), value.clone());
                }
                self.step_data = serde_json::Value::Object(combined_map);
            }
            _ => {
                self.step_data = data;
            }
        }
        self.updated_at = Utc::now();
    }

    /// Get data for a specific step
    pub fn get_step_data(&self, step: &str) -> Option<&serde_json::Value> {
        self.step_data.get(step)
    }
}

impl Default for WizardMode {
    fn default() -> Self {
        Self::Service
    }
}
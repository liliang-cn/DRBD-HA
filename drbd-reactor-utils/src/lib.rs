pub mod client;
pub mod error;
pub mod models;
pub mod parser;

// Re-export configuration types from config-gen (single source of truth)
pub use config_gen::{ConfigGenerator, ConfigPaths, OcfAgentConfig};

// Type aliases for backward compatibility
pub use config_gen::PromoterPluginConfig as PromoterConfig;
pub use config_gen::VipPluginConfig as VipConfig;

pub use client::DrbdReactorClient;
pub use error::Error;
pub use models::{EvictOptions, ReactorProfileStatus, ReactorServiceDetail, StatusOptions};

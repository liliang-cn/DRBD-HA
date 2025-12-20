//! HTTP API layer

pub mod cluster;
pub mod dashboard;
pub mod doc;
pub mod ha;
pub mod metrics;
pub mod middleware;
pub mod resource;
pub mod router;
pub mod sse;
pub mod storage;
pub mod ui;
pub mod wizard;

pub use router::create_router;

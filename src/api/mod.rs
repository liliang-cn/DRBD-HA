//! HTTP API layer

pub mod cluster;
pub mod ha;
pub mod middleware;
pub mod resource;
pub mod router;
pub mod sse;
pub mod ui;
pub mod storage;
pub mod dashboard;
pub mod doc;

pub use router::create_router;

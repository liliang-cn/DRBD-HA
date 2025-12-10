//! Prometheus metrics collection for DRBD-HA
//!
//! This module provides comprehensive metrics collection for monitoring
//! the health and performance of the DRBD-HA system.

use anyhow::Result;
use once_cell::sync::Lazy;
use prometheus::{
    Encoder, GaugeVec, Histogram, HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge,
    IntGaugeVec, Opts, Registry, TextEncoder,
};
use std::sync::Arc;

/// Metrics collector for DRBD-HA
#[derive(Clone)]
pub struct MetricsCollector {
    registry: Arc<Registry>,

    // System metrics
    pub nodes_total: IntGauge,
    pub nodes_online: IntGauge,
    pub resources_total: IntGauge,
    pub resources_healthy: IntGauge,
    pub resources_degraded: IntGauge,

    // DRBD specific metrics
    pub drbd_connections: IntGauge,
    pub drbd_sync_bytes_total: IntCounterVec,
    pub drbd_sync_progress: GaugeVec,
    pub drbd_split_brain_events: IntCounter,

    // HA profiles metrics
    pub ha_profiles_total: IntGauge,
    pub ha_profiles_active: IntGaugeVec,
    pub ha_profiles_standby: IntGaugeVec,
    pub ha_profiles_failed: IntGaugeVec,

    // Storage metrics
    pub storage_pools_total: IntGauge,
    pub storage_volumes_total: IntGauge,
    pub storage_used_bytes: GaugeVec,
    pub storage_free_bytes: GaugeVec,

    // API metrics
    pub api_requests_total: IntCounterVec,
    pub api_request_duration: HistogramVec,
    pub api_active_connections: IntGauge,

    // Operation metrics
    pub operations_total: IntCounterVec,
    pub operation_duration: HistogramVec,
    pub operation_failures: IntCounterVec,

    // Data migration metrics
    pub migration_operations_total: IntCounter,
    pub migration_bytes_transferred: IntCounter,
    pub migration_duration: Histogram,
}

impl MetricsCollector {
    /// Create a new metrics collector with default registry
    pub fn new() -> Result<Self> {
        let registry = Arc::new(Registry::new());
        Self::with_registry_internal(registry)
    }

    /// Create a new metrics collector with custom registry (for testing)
    pub fn with_registry(registry: Registry) -> Result<Self> {
        let registry = Arc::new(registry);
        Self::with_registry_internal(registry)
    }

    /// Internal constructor with Arc<Registry>
    fn with_registry_internal(registry: Arc<Registry>) -> Result<Self> {
        // System metrics
        let nodes_total = IntGauge::with_opts(Opts::new(
            "drbd_ha_nodes_total",
            "Total number of nodes in cluster",
        ))?;
        registry.register(Box::new(nodes_total.clone()))?;

        let nodes_online =
            IntGauge::with_opts(Opts::new("drbd_ha_nodes_online", "Number of online nodes"))?;
        registry.register(Box::new(nodes_online.clone()))?;

        let resources_total = IntGauge::with_opts(Opts::new(
            "drbd_ha_resources_total",
            "Total number of DRBD resources",
        ))?;
        registry.register(Box::new(resources_total.clone()))?;

        let resources_healthy = IntGauge::with_opts(Opts::new(
            "drbd_ha_resources_healthy",
            "Number of healthy DRBD resources",
        ))?;
        registry.register(Box::new(resources_healthy.clone()))?;

        let resources_degraded = IntGauge::with_opts(Opts::new(
            "drbd_ha_resources_degraded",
            "Number of degraded DRBD resources",
        ))?;
        registry.register(Box::new(resources_degraded.clone()))?;

        // DRBD specific metrics
        let drbd_connections = IntGauge::with_opts(Opts::new(
            "drbd_ha_drbd_connections",
            "Number of DRBD connections",
        ))?;
        registry.register(Box::new(drbd_connections.clone()))?;

        let drbd_sync_bytes_total = IntCounterVec::new(
            Opts::new(
                "drbd_ha_drbd_sync_bytes_total",
                "Total bytes synced by DRBD",
            ),
            &["resource"],
        )?;
        registry.register(Box::new(drbd_sync_bytes_total.clone()))?;

        let drbd_sync_progress = GaugeVec::new(
            Opts::new("drbd_ha_drbd_sync_progress", "DRBD sync progress (0-1)"),
            &["resource"],
        )?;
        registry.register(Box::new(drbd_sync_progress.clone()))?;

        let drbd_split_brain_events = IntCounter::with_opts(Opts::new(
            "drbd_ha_drbd_split_brain_events_total",
            "Total number of split-brain events",
        ))?;
        registry.register(Box::new(drbd_split_brain_events.clone()))?;

        // HA profiles metrics
        let ha_profiles_total = IntGauge::with_opts(Opts::new(
            "drbd_ha_profiles_total",
            "Total number of HA profiles",
        ))?;
        registry.register(Box::new(ha_profiles_total.clone()))?;

        let ha_profiles_active = IntGaugeVec::new(
            Opts::new("drbd_ha_profiles_active", "Number of active HA profiles"),
            &["profile_type"],
        )?;
        registry.register(Box::new(ha_profiles_active.clone()))?;

        let ha_profiles_standby = IntGaugeVec::new(
            Opts::new("drbd_ha_profiles_standby", "Number of standby HA profiles"),
            &["profile_type"],
        )?;
        registry.register(Box::new(ha_profiles_standby.clone()))?;

        let ha_profiles_failed = IntGaugeVec::new(
            Opts::new("drbd_ha_profiles_failed", "Number of failed HA profiles"),
            &["profile_type"],
        )?;
        registry.register(Box::new(ha_profiles_failed.clone()))?;

        // Storage metrics
        let storage_pools_total = IntGauge::with_opts(Opts::new(
            "drbd_ha_storage_pools_total",
            "Total number of storage pools",
        ))?;
        registry.register(Box::new(storage_pools_total.clone()))?;

        let storage_volumes_total = IntGauge::with_opts(Opts::new(
            "drbd_ha_storage_volumes_total",
            "Total number of storage volumes",
        ))?;
        registry.register(Box::new(storage_volumes_total.clone()))?;

        let storage_used_bytes = GaugeVec::new(
            Opts::new("drbd_ha_storage_used_bytes", "Used storage bytes"),
            &["pool"],
        )?;
        registry.register(Box::new(storage_used_bytes.clone()))?;

        let storage_free_bytes = GaugeVec::new(
            Opts::new("drbd_ha_storage_free_bytes", "Free storage bytes"),
            &["pool"],
        )?;
        registry.register(Box::new(storage_free_bytes.clone()))?;

        // API metrics
        let api_requests_total = IntCounterVec::new(
            Opts::new("drbd_ha_api_requests_total", "Total number of API requests"),
            &["method", "endpoint", "status"],
        )?;
        registry.register(Box::new(api_requests_total.clone()))?;

        let api_request_duration = HistogramVec::new(
            HistogramOpts::new(
                "drbd_ha_api_request_duration_seconds",
                "API request duration in seconds",
            ),
            &["method", "endpoint"],
        )?;
        registry.register(Box::new(api_request_duration.clone()))?;

        let api_active_connections = IntGauge::with_opts(Opts::new(
            "drbd_ha_api_active_connections",
            "Number of active API connections",
        ))?;
        registry.register(Box::new(api_active_connections.clone()))?;

        // Operation metrics
        let operations_total = IntCounterVec::new(
            Opts::new("drbd_ha_operations_total", "Total number of operations"),
            &["operation_type", "status"],
        )?;
        registry.register(Box::new(operations_total.clone()))?;

        let operation_duration = HistogramVec::new(
            HistogramOpts::new(
                "drbd_ha_operation_duration_seconds",
                "Operation duration in seconds",
            ),
            &["operation_type"],
        )?;
        registry.register(Box::new(operation_duration.clone()))?;

        let operation_failures = IntCounterVec::new(
            Opts::new(
                "drbd_ha_operation_failures_total",
                "Total number of operation failures",
            ),
            &["operation_type", "error_type"],
        )?;
        registry.register(Box::new(operation_failures.clone()))?;

        // Data migration metrics
        let migration_operations_total = IntCounter::with_opts(Opts::new(
            "drbd_ha_migration_operations_total",
            "Total number of migration operations",
        ))?;
        registry.register(Box::new(migration_operations_total.clone()))?;

        let migration_bytes_transferred = IntCounter::with_opts(Opts::new(
            "drbd_ha_migration_bytes_transferred_total",
            "Total bytes transferred during migrations",
        ))?;
        registry.register(Box::new(migration_bytes_transferred.clone()))?;

        let migration_duration = Histogram::with_opts(HistogramOpts::new(
            "drbd_ha_migration_duration_seconds",
            "Migration duration in seconds",
        ))?;
        registry.register(Box::new(migration_duration.clone()))?;

        Ok(Self {
            registry,
            nodes_total,
            nodes_online,
            resources_total,
            resources_healthy,
            resources_degraded,
            drbd_connections,
            drbd_sync_bytes_total,
            drbd_sync_progress,
            drbd_split_brain_events,
            ha_profiles_total,
            ha_profiles_active,
            ha_profiles_standby,
            ha_profiles_failed,
            storage_pools_total,
            storage_volumes_total,
            storage_used_bytes,
            storage_free_bytes,
            api_requests_total,
            api_request_duration,
            api_active_connections,
            operations_total,
            operation_duration,
            operation_failures,
            migration_operations_total,
            migration_bytes_transferred,
            migration_duration,
        })
    }

    /// Get the metrics registry
    pub fn registry(&self) -> Arc<Registry> {
        Arc::clone(&self.registry)
    }

    /// Export metrics as text for Prometheus
    pub fn export(&self) -> Result<String> {
        let metric_families = self.registry.gather();
        let encoder = TextEncoder::new();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8(buffer)?)
    }

    /// Update system metrics
    pub fn update_system_metrics(
        &self,
        nodes: usize,
        online: usize,
        resources: usize,
        healthy: usize,
        degraded: usize,
    ) {
        self.nodes_total.set(nodes as i64);
        self.nodes_online.set(online as i64);
        self.resources_total.set(resources as i64);
        self.resources_healthy.set(healthy as i64);
        self.resources_degraded.set(degraded as i64);
    }

    /// Update DRBD metrics
    pub fn update_drbd_metrics(&self, connections: usize) {
        self.drbd_connections.set(connections as i64);
    }

    /// Update DRBD resource sync progress
    pub fn update_drbd_sync_progress(&self, resource: &str, progress: f64) {
        self.drbd_sync_progress
            .with_label_values(&[resource])
            .set(progress);
    }

    /// Increment DRBD sync bytes
    pub fn inc_drbd_sync_bytes(&self, resource: &str, bytes: u64) {
        self.drbd_sync_bytes_total
            .with_label_values(&[resource])
            .inc_by(bytes);
    }

    /// Record split-brain event
    pub fn inc_split_brain_events(&self) {
        self.drbd_split_brain_events.inc();
    }

    /// Update HA profiles metrics
    pub fn update_ha_profiles_metrics(
        &self,
        total: usize,
        active_by_type: &[(&str, usize)],
        standby_by_type: &[(&str, usize)],
        failed_by_type: &[(&str, usize)],
    ) {
        self.ha_profiles_total.set(total as i64);

        for (profile_type, count) in active_by_type {
            self.ha_profiles_active
                .with_label_values(&[profile_type])
                .set(*count as i64);
        }

        for (profile_type, count) in standby_by_type {
            self.ha_profiles_standby
                .with_label_values(&[profile_type])
                .set(*count as i64);
        }

        for (profile_type, count) in failed_by_type {
            self.ha_profiles_failed
                .with_label_values(&[profile_type])
                .set(*count as i64);
        }
    }

    /// Update storage metrics
    pub fn update_storage_metrics(
        &self,
        pools: usize,
        volumes: usize,
        pool_usage: &[(&str, u64, u64)],
    ) {
        self.storage_pools_total.set(pools as i64);
        self.storage_volumes_total.set(volumes as i64);

        for (pool_name, used, free) in pool_usage {
            self.storage_used_bytes
                .with_label_values(&[pool_name])
                .set(*used as f64);
            self.storage_free_bytes
                .with_label_values(&[pool_name])
                .set(*free as f64);
        }
    }

    /// Record API request
    pub fn record_api_request(&self, method: &str, endpoint: &str, status: u16, duration: f64) {
        self.api_requests_total
            .with_label_values(&[method, endpoint, &status.to_string()])
            .inc();
        self.api_request_duration
            .with_label_values(&[method, endpoint])
            .observe(duration);
    }

    /// Increment active connections
    pub fn inc_active_connections(&self) {
        self.api_active_connections.inc();
    }

    /// Decrement active connections
    pub fn dec_active_connections(&self) {
        self.api_active_connections.dec();
    }

    /// Record operation
    pub fn record_operation(&self, operation_type: &str, status: &str, duration: f64) {
        self.operations_total
            .with_label_values(&[operation_type, status])
            .inc();
        self.operation_duration
            .with_label_values(&[operation_type])
            .observe(duration);
    }

    /// Record operation failure
    pub fn record_operation_failure(&self, operation_type: &str, error_type: &str) {
        self.operation_failures
            .with_label_values(&[operation_type, error_type])
            .inc();
    }

    /// Record migration operation
    pub fn record_migration(&self, bytes: u64, duration: f64) {
        self.migration_operations_total.inc();
        self.migration_bytes_transferred.inc_by(bytes);
        self.migration_duration.observe(duration);
    }
}

/// Global metrics collector instance
pub static METRICS: Lazy<MetricsCollector> =
    Lazy::new(|| MetricsCollector::new().expect("Failed to initialize metrics collector"));

/// Helper macro to record operation duration
#[macro_export]
macro_rules! record_operation {
    ($metrics:expr, $operation_type:expr, $operation:block) => {{
        let start = std::time::Instant::now();
        let result = $operation;
        let duration = start.elapsed().as_secs_f64();

        match &result {
            Ok(_) => {
                $metrics.record_operation($operation_type, "success", duration);
            }
            Err(e) => {
                let error_type = std::any::type_name_of_val(e);
                $metrics.record_operation_failure($operation_type, error_type);
                $metrics.record_operation($operation_type, "error", duration);
            }
        }

        result
    }};
}

/// Helper macro to record API request duration
#[macro_export]
macro_rules! record_api_request {
    ($metrics:expr, $method:expr, $endpoint:expr, $request:block) => {{
        let start = std::time::Instant::now();
        $metrics.inc_active_connections();

        let result = $request;
        let duration = start.elapsed().as_secs_f64();

        let status = match &result {
            Ok(_) => 200,
            Err(e) => {
                // Try to extract HTTP status from error, default to 500
                if let Some(_app_err) = e.downcast_ref::<$crate::error::AppError>() {
                    // Extract status from AppError if available
                    500 // Default for now, could be enhanced
                } else {
                    500
                }
            }
        };

        $metrics.record_api_request($method, $endpoint, status, duration);
        $metrics.dec_active_connections();

        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus::Registry;

    #[test]
    fn test_metrics_collector_creation() {
        // Use a custom registry to avoid conflicts
        let registry = Registry::new();
        let collector = MetricsCollector::with_registry(registry);
        assert!(collector.is_ok());
    }

    #[test]
    fn test_metrics_export() {
        let registry = Registry::new();
        let collector = MetricsCollector::with_registry(registry).unwrap();
        collector.update_system_metrics(3, 2, 5, 4, 1);

        let metrics_text = collector.export();
        assert!(metrics_text.is_ok());

        let text = metrics_text.unwrap();
        assert!(text.contains("drbd_ha_nodes_total"));
        assert!(text.contains("drbd_ha_nodes_online"));
    }

    #[test]
    fn test_api_request_recording() {
        let registry = Registry::new();
        let collector = MetricsCollector::with_registry(registry).unwrap();

        collector.record_api_request("GET", "/api/v1/nodes", 200, 0.1);
        collector.record_api_request("POST", "/api/v1/nodes", 201, 0.2);

        // Should not panic and metrics should be recorded
        let metrics_text = collector.export().unwrap();
        assert!(metrics_text.contains("drbd_ha_api_requests_total"));
        assert!(metrics_text.contains("drbd_ha_api_request_duration_seconds"));
    }

    #[test]
    fn test_operation_recording() {
        let registry = Registry::new();
        let collector = MetricsCollector::with_registry(registry).unwrap();

        collector.record_operation("create_resource", "success", 1.5);
        collector.record_operation("create_resource", "error", 0.5);
        collector.record_operation_failure("create_resource", "ValidationError");

        let metrics_text = collector.export().unwrap();
        assert!(metrics_text.contains("drbd_ha_operations_total"));
        assert!(metrics_text.contains("drbd_ha_operation_failures_total"));
    }
}

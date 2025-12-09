# Prometheus Metrics Integration

This document describes the Prometheus metrics integration in DRBD-HA Manager.

## Overview

The DRBD-HA Manager provides comprehensive metrics that can be scraped by Prometheus for monitoring and alerting. Metrics are exposed via HTTP endpoints in both Prometheus text format and JSON format.

## Available Metrics

### System Metrics
- `drbd_ha_nodes_total` - Total number of nodes in the cluster
- `drbd_ha_nodes_online` - Number of online nodes
- `drbd_ha_resources_total` - Total number of DRBD resources
- `drbd_ha_resources_healthy` - Number of healthy DRBD resources
- `drbd_ha_resources_degraded` - Number of degraded DRBD resources

### DRBD Metrics
- `drbd_ha_drbd_connections` - Number of DRBD connections
- `drbd_ha_drbd_sync_bytes_total` - Total bytes synced by DRBD (labeled by resource)
- `drbd_ha_drbd_sync_progress` - DRBD sync progress (0-1) (labeled by resource)
- `drbd_ha_drbd_split_brain_events_total` - Total number of split-brain events

### HA Profiles Metrics
- `drbd_ha_profiles_total` - Total number of HA profiles
- `drbd_ha_profiles_active` - Number of active HA profiles (labeled by profile_type)
- `drbd_ha_profiles_standby` - Number of standby HA profiles (labeled by profile_type)
- `drbd_ha_profiles_failed` - Number of failed HA profiles (labeled by profile_type)

### Storage Metrics
- `drbd_ha_storage_pools_total` - Total number of storage pools
- `drbd_ha_storage_volumes_total` - Total number of storage volumes
- `drbd_ha_storage_used_bytes` - Used storage bytes (labeled by pool)
- `drbd_ha_storage_free_bytes` - Free storage bytes (labeled by pool)

### API Metrics
- `drbd_ha_api_requests_total` - Total number of API requests (labeled by method, endpoint, status)
- `drbd_ha_api_request_duration_seconds` - API request duration in seconds (labeled by method, endpoint)
- `drbd_ha_api_active_connections` - Number of active API connections

### Operation Metrics
- `drbd_ha_operations_total` - Total number of operations (labeled by operation_type, status)
- `drbd_ha_operation_duration_seconds` - Operation duration in seconds (labeled by operation_type)
- `drbd_ha_operation_failures_total` - Total number of operation failures (labeled by operation_type, error_type)

### Data Migration Metrics
- `drbd_ha_migration_operations_total` - Total number of migration operations
- `drbd_ha_migration_bytes_transferred_total` - Total bytes transferred during migrations
- `drbd_ha_migration_duration_seconds` - Migration duration in seconds

## API Endpoints

### `/metrics`
Returns metrics in Prometheus text format for scraping by Prometheus server.

**Example:**
```bash
curl http://localhost:8080/metrics
```

**Response format:**
```
# HELP drbd_ha_nodes_total Total number of nodes in cluster
# TYPE drbd_ha_nodes_total gauge
drbd_ha_nodes_total 3
# HELP drbd_ha_nodes_online Number of online nodes
# TYPE drbd_ha_nodes_online gauge
drbd_ha_nodes_online 2
```

### `/api/v1/metrics/summary`
Returns a JSON summary of key metrics for dashboard display.

**Example:**
```bash
curl http://localhost:8080/api/v1/metrics/summary
```

**Response format:**
```json
{
  "timestamp": "2023-12-09T15:30:00Z",
  "system": {
    "nodes_total": "N/A",
    "nodes_online": "N/A",
    "resources_total": "N/A",
    "resources_healthy": "N/A",
    "resources_degraded": "N/A"
  },
  "drbd": {
    "connections": "N/A",
    "sync_bytes_total": "N/A"
  },
  "ha_profiles": {
    "total": "N/A",
    "active": "N/A",
    "standby": "N/A",
    "failed": "N/A"
  },
  "storage": {
    "pools_total": "N/A",
    "volumes_total": "N/A"
  },
  "api": {
    "requests_total": "N/A",
    "active_connections": "N/A"
  }
}
```

### `/api/v1/health/metrics`
Returns health status with basic metrics information.

**Example:**
```bash
curl http://localhost:8080/api/v1/health/metrics
```

**Response format:**
```json
{
  "status": "ok",
  "timestamp": "2023-12-09T15:30:00Z",
  "version": "0.1.0",
  "metrics_available": true,
  "uptime_seconds": "N/A",
  "memory_usage": "N/A",
  "cpu_usage": "N/A"
}
```

## Prometheus Configuration

Add the following to your Prometheus configuration to scrape metrics from DRBD-HA Manager:

```yaml
scrape_configs:
  - job_name: 'drbd-ha'
    static_configs:
      - targets: ['localhost:8080']
    metrics_path: /metrics
    scrape_interval: 15s
```

## Grafana Dashboard

You can create a Grafana dashboard to visualize the metrics. Here are some example panel configurations:

### Node Status
- Metric: `drbd_ha_nodes_online` / `drbd_ha_nodes_total`
- Visualization: Stat panel

### Resource Health
- Metric: `drbd_ha_resources_healthy` / `drbd_ha_resources_total`
- Visualization: Pie chart

### API Request Rate
- Metric: `rate(drbd_ha_api_requests_total[5m])`
- Visualization: Time series graph

### API Response Time
- Metric: `histogram_quantile(0.95, rate(drbd_ha_api_request_duration_seconds_bucket[5m]))`
- Visualization: Time series graph

## Using Metrics in Code

You can record custom metrics using the global `METRICS` instance:

```rust
use drbd_ha::core::metrics::METRICS;

// Record an API request
METRICS.record_api_request("GET", "/api/v1/nodes", 200, 0.1);

// Update system metrics
METRICS.update_system_metrics(3, 2, 5, 4, 1);

// Record operation with timing
let result = record_operation!(METRICS, "create_resource", {
    // Your operation code here
    create_resource_impl()
});
```

## Alerting Rules

Example Prometheus alerting rules:

```yaml
groups:
  - name: drbd-ha
    rules:
      - alert: NodeDown
        expr: drbd_ha_nodes_online < drbd_ha_nodes_total
        for: 1m
        labels:
          severity: warning
        annotations:
          summary: "DRBD-HA node is down"
          description: "One or more nodes in the DRBD-HA cluster are offline."

      - alert: DegradedResources
        expr: drbd_ha_resources_degraded > 0
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "DRBD resources are degraded"
          description: "One or more DRBD resources are in degraded state."

      - alert: HighAPILatency
        expr: histogram_quantile(0.95, rate(drbd_ha_api_request_duration_seconds_bucket[5m])) > 1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High API latency"
          description: "95th percentile API latency is above 1 second."
```

## Troubleshooting

### Metrics Not Available
- Check that the application is running
- Verify the metrics endpoint is accessible: `curl http://localhost:8080/metrics`
- Check application logs for any errors related to metrics initialization

### Missing Metrics
- Some metrics may only be populated when certain operations are performed
- Check that the relevant features are being used in the application
- Verify that metric updates are being called in the code

### High Memory Usage
- Prometheus metrics can consume memory, especially with high-cardinality labels
- Consider reducing label cardinality or metric retention time
- Monitor the `drbd_ha_metrics_*` metrics if available

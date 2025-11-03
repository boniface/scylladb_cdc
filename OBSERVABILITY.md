# Observability Guide

This application includes comprehensive logging, tracing, and metrics to measure and monitor performance, including latency percentiles (p50, p95, p99).

## Table of Contents
- [Metrics Overview](#metrics-overview)
- [Accessing Metrics](#accessing-metrics)
- [Latency Metrics (p50, p95, p99)](#latency-metrics-p50-p95-p99)
- [Prometheus Queries](#prometheus-queries)
- [Tracing & Logging](#tracing--logging)
- [Integration with Monitoring Tools](#integration-with-monitoring-tools)

## Metrics Overview

The application exposes Prometheus metrics on port **9090** at the `/metrics` endpoint.

### Available Metrics

#### CDC Processing Metrics
- `cdc_events_processed_total` - Counter of successfully processed CDC events (by event_type)
- `cdc_events_failed_total` - Counter of failed CDC events (by event_type, reason)
- `cdc_processing_duration_seconds` - **Histogram** of CDC processing latency (by event_type)
  - Buckets: 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0 seconds

#### Event Store Metrics
- `event_store_append_duration_seconds` - **Histogram** of event store append latency (by aggregate_type)
  - Buckets: 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0 seconds
- `event_store_load_duration_seconds` - **Histogram** of event store load latency (by aggregate_type)
  - Buckets: 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0 seconds
- `events_appended_total` - Counter of events appended to event store (by aggregate_type)
- `events_loaded_total` - Counter of events loaded from event store (by aggregate_type)

#### Retry Metrics
- `retry_attempts_total` - Counter of retry attempts (by operation, attempt)
- `retry_success_total` - Counter of successful retries (by operation)
- `retry_failure_total` - Counter of failed retries after all attempts (by operation)

#### DLQ (Dead Letter Queue) Metrics
- `dlq_messages_total` - Counter of total messages in DLQ
- `dlq_messages_by_event_type` - Counter of DLQ messages by event type

#### Circuit Breaker Metrics
- `circuit_breaker_state` - Gauge of circuit breaker state (0=Closed, 1=Open, 2=HalfOpen)
- `circuit_breaker_transitions_total` - Counter of circuit breaker state transitions

#### Actor Metrics
- `actor_health_status` - Gauge of actor health (0=Unhealthy, 1=Degraded, 2=Healthy)
- `actor_messages_sent_total` - Counter of messages sent by actors
- `actor_messages_received_total` - Counter of messages received by actors

## Accessing Metrics

### Direct Access
```bash
# Get all metrics
curl http://localhost:9090/metrics

# Health check
curl http://localhost:9090/health
```

### Sample Output
```
# HELP cdc_processing_duration_seconds CDC event processing duration
# TYPE cdc_processing_duration_seconds histogram
cdc_processing_duration_seconds_bucket{event_type="OrderCreated",le="0.001"} 0
cdc_processing_duration_seconds_bucket{event_type="OrderCreated",le="0.005"} 2
cdc_processing_duration_seconds_bucket{event_type="OrderCreated",le="0.01"} 15
cdc_processing_duration_seconds_bucket{event_type="OrderCreated",le="0.05"} 45
cdc_processing_duration_seconds_bucket{event_type="OrderCreated",le="+Inf"} 50
cdc_processing_duration_seconds_sum{event_type="OrderCreated"} 1.234
cdc_processing_duration_seconds_count{event_type="OrderCreated"} 50
```

## Latency Metrics (p50, p95, p99)

Prometheus histograms automatically track percentiles. Use the `histogram_quantile()` function to calculate p50, p95, and p99 latencies.

### Calculating Percentiles

#### CDC Processing Latency

**p50 (Median) - 50th percentile:**
```promql
histogram_quantile(0.50,
  rate(cdc_processing_duration_seconds_bucket[5m])
)
```

**p95 - 95th percentile:**
```promql
histogram_quantile(0.95,
  rate(cdc_processing_duration_seconds_bucket[5m])
)
```

**p99 - 99th percentile:**
```promql
histogram_quantile(0.99,
  rate(cdc_processing_duration_seconds_bucket[5m])
)
```

**By event type:**
```promql
histogram_quantile(0.95,
  sum by (event_type, le) (
    rate(cdc_processing_duration_seconds_bucket[5m])
  )
)
```

#### Event Store Append Latency

**p95 for event store append operations:**
```promql
histogram_quantile(0.95,
  rate(event_store_append_duration_seconds_bucket[5m])
)
```

**By aggregate type:**
```promql
histogram_quantile(0.99,
  sum by (aggregate_type, le) (
    rate(event_store_append_duration_seconds_bucket[5m])
  )
)
```

#### Event Store Load Latency

**p95 for event store load operations:**
```promql
histogram_quantile(0.95,
  rate(event_store_load_duration_seconds_bucket[5m])
)
```

## Prometheus Queries

### Throughput Metrics

**CDC Events processed per second:**
```promql
rate(cdc_events_processed_total[1m])
```

**Events appended per second:**
```promql
rate(events_appended_total[1m])
```

### Error Rates

**CDC failure rate:**
```promql
rate(cdc_events_failed_total[5m])
/
rate(cdc_events_processed_total[5m])
```

**Retry failure rate:**
```promql
sum(rate(retry_failure_total[5m])) by (operation)
```

### DLQ Monitoring

**DLQ growth rate:**
```promql
rate(dlq_messages_total[5m])
```

**DLQ messages by event type:**
```promql
dlq_messages_by_event_type
```

### Comprehensive Latency Dashboard

**Average latency over time:**
```promql
rate(cdc_processing_duration_seconds_sum[5m])
/
rate(cdc_processing_duration_seconds_count[5m])
```

**All percentiles in one view:**
```promql
# p50
histogram_quantile(0.50, rate(cdc_processing_duration_seconds_bucket[5m]))

# p90
histogram_quantile(0.90, rate(cdc_processing_duration_seconds_bucket[5m]))

# p95
histogram_quantile(0.95, rate(cdc_processing_duration_seconds_bucket[5m]))

# p99
histogram_quantile(0.99, rate(cdc_processing_duration_seconds_bucket[5m]))

# p99.9
histogram_quantile(0.999, rate(cdc_processing_duration_seconds_bucket[5m]))
```

## Tracing & Logging

### Structured Logging

The application uses `tracing` for structured logging with contextual information.

**Log Levels:**
- `ERROR` - Critical failures, DLQ messages
- `WARN` - Degraded health, retries
- `INFO` - Normal operations, successful events
- `DEBUG` - Detailed operation traces
- `TRACE` - Very detailed debugging

**Configure log level:**
```bash
# Via environment variable
RUST_LOG=scylladb_cdc=debug cargo run

# Default level
RUST_LOG=info,scylladb_cdc=debug cargo run
```

### Tracing Spans

Key operations are instrumented with tracing spans that include:

**CDC Processing:**
- Stream ID
- Operation type
- Event ID, type, and aggregate ID
- Duration in milliseconds

**Event Store Operations:**
- Aggregate ID and type
- Expected version
- Event count
- Duration in milliseconds

### Sample Log Output

```
2025-11-02T10:15:23.456Z INFO scylladb_cdc::actors::infrastructure::cdc_processor:
  event_id=550e8400-e29b-41d4-a716-446655440000
  event_type="OrderCreated"
  aggregate_id=7c9e6679-7425-40de-944b-e07fc1f90ae7
  duration_ms=12.5
  Successfully published event via CDC stream

2025-11-02T10:15:23.468Z INFO scylladb_cdc::event_sourcing::store::event_store:
  aggregate_id=7c9e6679-7425-40de-944b-e07fc1f90ae7
  aggregate_type="Order"
  new_version=1
  event_count=1
  duration_ms=8.3
  Appended events to event store
```

## Integration with Monitoring Tools

### Prometheus

1. **Add scrape target to `prometheus.yml`:**
```yaml
scrape_configs:
  - job_name: 'scylladb_cdc'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 15s
```

2. **Run Prometheus:**
```bash
prometheus --config.file=prometheus.yml
```

3. **Access Prometheus UI:**
```
http://localhost:9090
```

### Grafana

1. **Add Prometheus data source** (http://prometheus:9090)

2. **Create dashboards with panels:**

**CDC Processing Latency Panel:**
- Query: `histogram_quantile(0.95, rate(cdc_processing_duration_seconds_bucket[5m]))`
- Visualization: Time series
- Legend: `p95 CDC Latency`

**Event Store Performance Panel:**
- Multiple queries for p50, p95, p99 of append/load operations
- Visualization: Graph with multiple series

**Throughput Panel:**
- Query: `rate(cdc_events_processed_total[1m])`
- Visualization: Graph
- Unit: ops/sec

**Error Rate Panel:**
- Query: `rate(cdc_events_failed_total[5m]) / rate(cdc_events_processed_total[5m])`
- Visualization: Graph
- Unit: Percentage

### Sample Grafana Dashboard JSON

```json
{
  "dashboard": {
    "title": "ScyllaDB CDC Observability",
    "panels": [
      {
        "title": "CDC Processing Latency (p95)",
        "targets": [
          {
            "expr": "histogram_quantile(0.95, rate(cdc_processing_duration_seconds_bucket[5m]))"
          }
        ]
      },
      {
        "title": "Event Store Append Latency Percentiles",
        "targets": [
          {
            "expr": "histogram_quantile(0.50, rate(event_store_append_duration_seconds_bucket[5m]))",
            "legendFormat": "p50"
          },
          {
            "expr": "histogram_quantile(0.95, rate(event_store_append_duration_seconds_bucket[5m]))",
            "legendFormat": "p95"
          },
          {
            "expr": "histogram_quantile(0.99, rate(event_store_append_duration_seconds_bucket[5m]))",
            "legendFormat": "p99"
          }
        ]
      }
    ]
  }
}
```

## Performance Tuning

### Histogram Bucket Configuration

If your latencies fall outside the current buckets, adjust them in `src/metrics/mod.rs`:

```rust
// For higher latencies
.buckets(vec![0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0])

// For lower latencies (microsecond precision)
.buckets(vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1])
```

### Monitoring Best Practices

1. **Set up alerts** for:
   - p95 latency > threshold (e.g., 100ms)
   - Error rate > 1%
   - DLQ growth rate > 0
   - Circuit breaker open state

2. **Create SLOs/SLIs:**
   - 99% of requests < 50ms (p99 latency)
   - 99.9% availability
   - Error rate < 0.1%

3. **Use recording rules** in Prometheus for expensive queries:
```yaml
groups:
  - name: latency_percentiles
    interval: 30s
    rules:
      - record: cdc:latency:p95
        expr: histogram_quantile(0.95, rate(cdc_processing_duration_seconds_bucket[5m]))
```

## Troubleshooting

### No metrics appearing?

1. Check metrics server is running:
```bash
curl http://localhost:9090/health
```

2. Verify metrics are being recorded:
```bash
curl http://localhost:9090/metrics | grep cdc_processing
```

3. Check logs for errors:
```bash
RUST_LOG=debug cargo run
```

### Histogram buckets not matching your workload?

Update bucket boundaries in `src/metrics/mod.rs` to match your actual latency distribution.

### Want more detailed traces?

Enable debug/trace logging:
```bash
RUST_LOG=scylladb_cdc=trace cargo run
```

---

For more information on Prometheus histograms and quantiles:
- [Prometheus Histogram Documentation](https://prometheus.io/docs/practices/histograms/)
- [Grafana Dashboard Examples](https://grafana.com/grafana/dashboards/)

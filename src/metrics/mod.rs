// Private module declaration
mod server;

use prometheus::{
    HistogramOpts, HistogramVec, IntCounter, IntCounterVec,
    IntGauge, Opts, Registry,
};

// Re-export for public API
pub use server::start_metrics_server;

// ============================================================================
// Metrics Module - Prometheus metrics for observability
// ============================================================================
//
// Provides comprehensive metrics for:
// - CDC event processing (throughput, latency)
// - Retry attempts and outcomes
// - Dead Letter Queue statistics
// - Circuit breaker state transitions
// - Actor health status
//
// All metrics are registered with Prometheus and can be scraped via /metrics
// ============================================================================

/// Central metrics registry for the entire application
#[allow(dead_code)]
pub struct Metrics {
    registry: Registry,

    // CDC Processing Metrics
    pub cdc_events_processed: IntCounterVec,
    pub cdc_events_failed: IntCounterVec,
    pub cdc_processing_duration: HistogramVec,

    // Retry Metrics
    pub retry_attempts_total: IntCounterVec,
    pub retry_success: IntCounterVec,
    pub retry_failure: IntCounterVec,

    // DLQ Metrics
    pub dlq_messages_total: IntCounter,
    pub dlq_messages_by_event_type: IntCounterVec,

    // Circuit Breaker Metrics
    pub circuit_breaker_state: IntGauge,
    pub circuit_breaker_transitions: IntCounterVec,

    // Actor Metrics
    pub actor_health_status: IntGauge,
    pub messages_sent: IntCounterVec,
    pub messages_received: IntCounterVec,
}

impl Metrics {
    #[allow(dead_code)]
    pub fn new() -> anyhow::Result<Self> {
        let registry = Registry::new();

        // CDC Processing Metrics
        let cdc_events_processed = IntCounterVec::new(
            Opts::new("cdc_events_processed_total", "Total CDC events processed"),
            &["event_type"],
        )?;
        registry.register(Box::new(cdc_events_processed.clone()))?;

        let cdc_events_failed = IntCounterVec::new(
            Opts::new("cdc_events_failed_total", "Total CDC events that failed processing"),
            &["event_type", "reason"],
        )?;
        registry.register(Box::new(cdc_events_failed.clone()))?;

        let cdc_processing_duration = HistogramVec::new(
            HistogramOpts::new("cdc_processing_duration_seconds", "CDC event processing duration")
                .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]),
            &["event_type"],
        )?;
        registry.register(Box::new(cdc_processing_duration.clone()))?;

        // Retry Metrics
        let retry_attempts_total = IntCounterVec::new(
            Opts::new("retry_attempts_total", "Total retry attempts"),
            &["operation", "attempt"],
        )?;
        registry.register(Box::new(retry_attempts_total.clone()))?;

        let retry_success = IntCounterVec::new(
            Opts::new("retry_success_total", "Total successful retries"),
            &["operation"],
        )?;
        registry.register(Box::new(retry_success.clone()))?;

        let retry_failure = IntCounterVec::new(
            Opts::new("retry_failure_total", "Total failed retries after all attempts"),
            &["operation"],
        )?;
        registry.register(Box::new(retry_failure.clone()))?;

        // DLQ Metrics
        let dlq_messages_total = IntCounter::new(
            "dlq_messages_total",
            "Total messages in dead letter queue",
        )?;
        registry.register(Box::new(dlq_messages_total.clone()))?;

        let dlq_messages_by_event_type = IntCounterVec::new(
            Opts::new("dlq_messages_by_event_type", "DLQ messages by event type"),
            &["event_type"],
        )?;
        registry.register(Box::new(dlq_messages_by_event_type.clone()))?;

        // Circuit Breaker Metrics
        let circuit_breaker_state = IntGauge::new(
            "circuit_breaker_state",
            "Circuit breaker state (0=Closed, 1=Open, 2=HalfOpen)",
        )?;
        registry.register(Box::new(circuit_breaker_state.clone()))?;

        let circuit_breaker_transitions = IntCounterVec::new(
            Opts::new("circuit_breaker_transitions_total", "Circuit breaker state transitions"),
            &["from_state", "to_state"],
        )?;
        registry.register(Box::new(circuit_breaker_transitions.clone()))?;

        // Actor Metrics
        let actor_health_status = IntGauge::new(
            "actor_health_status",
            "Actor health status (0=Unhealthy, 1=Degraded, 2=Healthy)",
        )?;
        registry.register(Box::new(actor_health_status.clone()))?;

        let messages_sent = IntCounterVec::new(
            Opts::new("actor_messages_sent_total", "Total messages sent by actors"),
            &["actor", "message_type"],
        )?;
        registry.register(Box::new(messages_sent.clone()))?;

        let messages_received = IntCounterVec::new(
            Opts::new("actor_messages_received_total", "Total messages received by actors"),
            &["actor", "message_type"],
        )?;
        registry.register(Box::new(messages_received.clone()))?;

        Ok(Self {
            registry,
            cdc_events_processed,
            cdc_events_failed,
            cdc_processing_duration,
            retry_attempts_total,
            retry_success,
            retry_failure,
            dlq_messages_total,
            dlq_messages_by_event_type,
            circuit_breaker_state,
            circuit_breaker_transitions,
            actor_health_status,
            messages_sent,
            messages_received,
        })
    }

    /// Get the Prometheus registry for exposing metrics via HTTP
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Helper to record CDC event processing
    pub fn record_cdc_event(&self, event_type: &str, duration_secs: f64, success: bool) {
        if success {
            self.cdc_events_processed.with_label_values(&[event_type]).inc();
        } else {
            self.cdc_events_failed.with_label_values(&[event_type, "processing_error"]).inc();
        }
        self.cdc_processing_duration.with_label_values(&[event_type]).observe(duration_secs);
    }

    /// Helper to record retry attempt
    pub fn record_retry_attempt(&self, operation: &str, attempt: u32) {
        self.retry_attempts_total.with_label_values(&[operation, &attempt.to_string()]).inc();
    }

    /// Helper to record retry outcome
    pub fn record_retry_outcome(&self, operation: &str, success: bool) {
        if success {
            self.retry_success.with_label_values(&[operation]).inc();
        } else {
            self.retry_failure.with_label_values(&[operation]).inc();
        }
    }

    /// Helper to record DLQ message
    pub fn record_dlq_message(&self, event_type: &str) {
        self.dlq_messages_total.inc();
        self.dlq_messages_by_event_type.with_label_values(&[event_type]).inc();
    }

    /// Helper to update circuit breaker state
    pub fn update_circuit_breaker_state(&self, state: u8) {
        self.circuit_breaker_state.set(state as i64);
    }

    /// Helper to record circuit breaker transition
    pub fn record_circuit_breaker_transition(&self, from_state: &str, to_state: &str) {
        self.circuit_breaker_transitions.with_label_values(&[from_state, to_state]).inc();
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new().expect("Failed to create metrics")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = Metrics::new().unwrap();
        assert!(metrics.registry.gather().len() > 0);
    }

    #[test]
    fn test_record_cdc_event() {
        let metrics = Metrics::new().unwrap();
        metrics.record_cdc_event("OrderCreated", 0.05, true);

        let gathered = metrics.registry.gather();
        let processed = gathered.iter().find(|m| m.name() == "cdc_events_processed_total").unwrap();
        assert_eq!(processed.metric[0].counter.value, Some(1.0));
    }

    #[test]
    fn test_record_retry() {
        let metrics = Metrics::new().unwrap();
        metrics.record_retry_attempt("redpanda_publish", 1);
        metrics.record_retry_attempt("redpanda_publish", 2);
        metrics.record_retry_outcome("redpanda_publish", true);

        let gathered = metrics.registry.gather();
        let attempts = gathered.iter().find(|m| m.name() == "retry_attempts_total").unwrap();
        assert_eq!(attempts.metric.len(), 2); // Two different attempt labels
    }

    #[test]
    fn test_record_dlq_message() {
        let metrics = Metrics::new().unwrap();
        metrics.record_dlq_message("OrderCreated");
        metrics.record_dlq_message("OrderUpdated");

        let gathered = metrics.registry.gather();
        let dlq_total = gathered.iter().find(|m| m.name() == "dlq_messages_total").unwrap();
        assert_eq!(dlq_total.metric[0].counter.value, Some(2.0));
    }

    #[test]
    fn test_circuit_breaker_metrics() {
        let metrics = Metrics::new().unwrap();
        metrics.update_circuit_breaker_state(0); // Closed
        metrics.record_circuit_breaker_transition("Closed", "Open");
        metrics.update_circuit_breaker_state(1); // Open

        let gathered = metrics.registry.gather();
        let state = gathered.iter().find(|m| m.name() == "circuit_breaker_state").unwrap();
        assert_eq!(state.metric[0].gauge.value, Some(1.0));
    }
}

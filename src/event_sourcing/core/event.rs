use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use anyhow::Result;

// ============================================================================
// Event Envelope - Industry Standard Event Metadata
// ============================================================================
//
// Wraps domain events with metadata for proper event sourcing.
// This is GENERIC and works with ANY event type.
//
// ============================================================================

/// Generic Event Envelope - wraps any domain event with metadata
///
/// Type Parameter:
/// - `E`: The domain event type (must implement DomainEvent trait)
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EventEnvelope<E> {
    // Event Identity
    pub event_id: Uuid,
    pub aggregate_id: Uuid,
    pub sequence_number: i64,

    // Event Type Information
    pub event_type: String,
    pub event_version: i32,

    // Event Payload
    pub event_data: E,

    // Causation & Correlation (for distributed tracing)
    pub causation_id: Option<Uuid>,      // What command/event caused this
    pub correlation_id: Uuid,            // Groups related events across aggregates

    // Actor Information
    pub user_id: Option<Uuid>,           // Who triggered this event

    // Timing
    pub timestamp: DateTime<Utc>,

    // Additional Metadata
    pub metadata: HashMap<String, String>,
}

impl<E> EventEnvelope<E> {
    pub fn new(
        aggregate_id: Uuid,
        sequence_number: i64,
        event_type: String,
        event_data: E,
        correlation_id: Uuid,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            aggregate_id,
            sequence_number,
            event_type,
            event_version: 1, // Start at version 1
            event_data,
            causation_id: None,
            correlation_id,
            user_id: None,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_user(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn with_causation(mut self, causation_id: Uuid) -> Self {
        self.causation_id = Some(causation_id);
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

// ============================================================================
// Domain Event Trait
// ============================================================================

/// Generic Domain Event trait
///
/// All domain events must implement this trait to be used with the event store.
pub trait DomainEvent: Serialize + for<'de> Deserialize<'de> + Clone + Send + Sync {
    fn event_type() -> &'static str where Self: Sized;
    fn event_version() -> i32 where Self: Sized { 1 }
}

// ============================================================================
// Event Serialization Helpers
// ============================================================================

pub fn serialize_event<E: Serialize>(event: &E) -> Result<String> {
    Ok(serde_json::to_string(event)?)
}

pub fn deserialize_event<E: for<'de> Deserialize<'de>>(json: &str) -> Result<E> {
    Ok(serde_json::from_str(json)?)
}

// ============================================================================
// Event Versioning Support
// ============================================================================

/// Upcaster trait for evolving event schemas
pub trait EventUpcaster {
    fn upcast(&self, from_version: i32, event_json: &str) -> Result<String>;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Clone, Debug)]
    struct TestEvent {
        data: String,
    }

    impl DomainEvent for TestEvent {
        fn event_type() -> &'static str { "TestEvent" }
    }

    #[test]
    fn test_event_envelope_creation() {
        let aggregate_id = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();

        let event = TestEvent {
            data: "test".to_string(),
        };

        let envelope = EventEnvelope::new(
            aggregate_id,
            1,
            TestEvent::event_type().to_string(),
            event,
            correlation_id,
        );

        assert_eq!(envelope.aggregate_id, aggregate_id);
        assert_eq!(envelope.sequence_number, 1);
        assert_eq!(envelope.event_type, "TestEvent");
        assert_eq!(envelope.correlation_id, correlation_id);
    }

    #[test]
    fn test_event_serialization() {
        let event = TestEvent {
            data: "test data".to_string(),
        };

        let json = serialize_event(&event).unwrap();
        let deserialized: TestEvent = deserialize_event(&json).unwrap();

        assert_eq!(event.data, deserialized.data);
    }
}

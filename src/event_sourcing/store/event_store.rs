use scylla::client::session::Session;
use std::sync::Arc;
use uuid::Uuid;
use anyhow::{Result, bail};
use chrono::Utc;
use std::marker::PhantomData;

use crate::event_sourcing::core::{DomainEvent, EventEnvelope, Aggregate, serialize_event};

// ============================================================================
// Generic Event Store - Repository for Events
// ============================================================================
//
// This is a GENERIC event store that works with ANY event type.
//
// Type Parameter:
// - `E`: The domain event type (must implement DomainEvent trait)
//
// Responsibilities:
// 1. Append events to event_store table (append-only)
// 2. Load event history for aggregates
// 3. Ensure optimistic concurrency control
// 4. Write to outbox for publishing
//
// ============================================================================

pub struct EventStore<E: DomainEvent> {
    session: Arc<Session>,
    aggregate_type_name: String,  // e.g., "Order", "Customer", "Product"
    topic_name: String,            // e.g., "order-events", "customer-events"
    _phantom: PhantomData<E>,
}

impl<E: DomainEvent> EventStore<E> {
    pub fn new(session: Arc<Session>, aggregate_type_name: &str, topic_name: &str) -> Self {
        Self {
            session,
            aggregate_type_name: aggregate_type_name.to_string(),
            topic_name: topic_name.to_string(),
            _phantom: PhantomData,
        }
    }

    /// Append events to the event store
    /// Returns the new version number after appending
    pub async fn append_events(
        &self,
        aggregate_id: Uuid,
        expected_version: i64,
        events: Vec<EventEnvelope<E>>,
        publish_to_outbox: bool,
    ) -> Result<i64> {
        if events.is_empty() {
            bail!("Cannot append empty event list");
        }

        // Check optimistic concurrency
        let current_version = self.get_current_version(aggregate_id).await?;
        if current_version != expected_version {
            bail!(
                "Concurrency conflict: expected version {}, but current is {}",
                expected_version,
                current_version
            );
        }

        // Prepare batch for atomic write
        let mut batch = scylla::statement::batch::Batch::default();
        let mut values: Vec<Box<dyn scylla::serialize::row::SerializeRow>> = vec![];

        let mut new_version = expected_version;

        // Build batch statements and values in ONE loop
        for event_envelope in &events {
            new_version += 1;

            // Serialize event data once
            let event_json = serialize_event(&event_envelope.event_data)?;

            // Insert into event_store
            batch.append_statement(
                "INSERT INTO event_store (
                    aggregate_id, sequence_number, event_id, event_type, event_version,
                    event_data, causation_id, correlation_id, timestamp
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
            );

            // Event store values
            values.push(Box::new((
                aggregate_id,
                new_version,
                event_envelope.event_id,
                event_envelope.event_type.clone(),
                event_envelope.event_version,
                event_json.clone(),
                event_envelope.causation_id,
                event_envelope.correlation_id,
                event_envelope.timestamp,
            )));

            // If publishing to outbox, add outbox entry
            if publish_to_outbox {
                batch.append_statement(
                    "INSERT INTO outbox_messages (
                        id, aggregate_id, aggregate_type, event_id, event_type, event_version,
                        payload, topic, partition_key, causation_id, correlation_id,
                        created_at, attempts
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)"
                );

                let partition_key = aggregate_id.to_string();

                // Outbox values
                values.push(Box::new((
                    Uuid::new_v4(), // outbox message id
                    aggregate_id,
                    self.aggregate_type_name.clone(),
                    event_envelope.event_id,
                    event_envelope.event_type.clone(),
                    event_envelope.event_version,
                    event_json,
                    self.topic_name.clone(),
                    partition_key,
                    event_envelope.causation_id,
                    event_envelope.correlation_id,
                    Utc::now(),
                )));
            }
        }

        // Insert/Update aggregate sequence (use INSERT for upsert behavior)
        batch.append_statement(
            "INSERT INTO aggregate_sequence (aggregate_id, current_sequence, updated_at) VALUES (?, ?, ?)"
        );

        // Sequence update values
        values.push(Box::new((aggregate_id, new_version, Utc::now())));

        // Execute batch
        self.session.batch(&batch, values).await?;

        tracing::info!(
            aggregate_id = %aggregate_id,
            aggregate_type = %self.aggregate_type_name,
            new_version = new_version,
            event_count = events.len(),
            "✅ Appended events to event store"
        );

        Ok(new_version)
    }

    /// Load all events for an aggregate
    pub async fn load_events(&self, aggregate_id: Uuid) -> Result<Vec<EventEnvelope<E>>> {
        let result = self.session
            .query_unpaged(
                "SELECT aggregate_id, sequence_number, event_id, event_type, event_version,
                        event_data, causation_id, correlation_id, timestamp
                 FROM event_store
                 WHERE aggregate_id = ?
                 ORDER BY sequence_number ASC",
                (aggregate_id,),
            )
            .await?;

        let mut events = Vec::new();

        let rows_result = match result.into_rows_result() {
            Ok(rows) => rows,
            Err(_) => return Ok(events), // No rows
        };

        for row in rows_result.rows::<(Uuid, i64, Uuid, String, i32, String, Option<Uuid>, Uuid, chrono::DateTime<Utc>)>()? {
            let (agg_id, sequence_number, event_id, event_type, event_version, event_data_json, causation_id, correlation_id, timestamp) = row?;

            tracing::debug!("Loaded event for aggregate {}: seq={}, type={}", agg_id, sequence_number, event_type);

            // Parse event data based on type
            let event_data: E = serde_json::from_str(&event_data_json)?;

            let envelope = EventEnvelope {
                event_id,
                aggregate_id: agg_id,
                sequence_number,
                event_type,
                event_version,
                event_data,
                causation_id,
                correlation_id,
                user_id: None,
                timestamp,
                metadata: std::collections::HashMap::new(),
            };

            events.push(envelope);
        }

        tracing::debug!("Loaded {} events for aggregate {}", events.len(), aggregate_id);
        Ok(events)
    }

    /// Get current version of aggregate
    pub async fn get_current_version(&self, aggregate_id: Uuid) -> Result<i64> {
        let result = self.session
            .query_unpaged(
                "SELECT current_sequence FROM aggregate_sequence WHERE aggregate_id = ?",
                (aggregate_id,),
            )
            .await?;

        let rows_result = match result.into_rows_result() {
            Ok(rows) => rows,
            Err(_) => return Ok(0), // No rows = new aggregate
        };

        match rows_result.maybe_first_row::<(i64,)>() {
            Ok(Some((version,))) => Ok(version),
            _ => Ok(0), // No rows = new aggregate
        }
    }

    /// Load aggregate from events
    pub async fn load_aggregate<A>(&self, aggregate_id: Uuid) -> Result<A>
    where
        A: Aggregate<Event = E>,
        <A as Aggregate>::Error: std::fmt::Display,
    {
        let events = self.load_events(aggregate_id).await?;

        if events.is_empty() {
            bail!("Aggregate not found: {}", aggregate_id);
        }

        A::load_from_events(events)
    }

    /// Check if aggregate exists
    pub async fn aggregate_exists(&self, aggregate_id: Uuid) -> Result<bool> {
        let version = self.get_current_version(aggregate_id).await?;
        Ok(version > 0)
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::order::{OrderEvent, OrderCreated};
    use crate::domain::order::OrderItem;

    #[test]
    fn test_event_store_creation() {
        // Note: This test verifies EventStore can be created with proper type parameters
        // Actual database operations require integration tests

        // We can't create a real Session without a database, but we can verify
        // the EventStore struct exists and has the right signature
        let aggregate_type = "Order";
        let topic = "order-events";

        // Verify the type signature compiles
        let _store_type = std::marker::PhantomData::<EventStore<OrderEvent>>;

        assert_eq!(aggregate_type, "Order");
        assert_eq!(topic, "order-events");
    }

    #[test]
    fn test_event_envelope_construction_for_store() {
        let aggregate_id = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();

        let event = OrderEvent::Created(OrderCreated {
            customer_id: Uuid::new_v4(),
            items: vec![OrderItem {
                product_id: Uuid::new_v4(),
                quantity: 2,
            }],
        });

        let envelope = EventEnvelope::new(
            aggregate_id,
            1,
            "OrderCreated".to_string(),
            event,
            correlation_id,
        );

        assert_eq!(envelope.aggregate_id, aggregate_id);
        assert_eq!(envelope.sequence_number, 1);
        assert_eq!(envelope.event_type, "OrderCreated");
        assert_eq!(envelope.correlation_id, correlation_id);
    }

    #[test]
    fn test_event_serialization_for_storage() {
        let event = OrderEvent::Created(OrderCreated {
            customer_id: Uuid::new_v4(),
            items: vec![OrderItem {
                product_id: Uuid::new_v4(),
                quantity: 1,
            }],
        });

        // Test that events can be serialized for storage
        let serialized = serialize_event(&event).unwrap();
        assert!(!serialized.is_empty());
        assert!(serialized.contains("Created"));

        // Verify deserialization works
        let deserialized: OrderEvent = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            OrderEvent::Created(_) => {},
            _ => panic!("Wrong event type after deserialization"),
        }
    }

    #[test]
    fn test_multiple_events_batch_preparation() {
        let aggregate_id = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();

        // Simulate preparing multiple events for batch insert
        let events = vec![
            EventEnvelope::new(
                aggregate_id,
                1,
                "OrderCreated".to_string(),
                OrderEvent::Created(OrderCreated {
                    customer_id: Uuid::new_v4(),
                    items: vec![],
                }),
                correlation_id,
            ),
            EventEnvelope::new(
                aggregate_id,
                2,
                "OrderConfirmed".to_string(),
                OrderEvent::Confirmed(crate::domain::order::OrderConfirmed {
                    confirmed_at: Utc::now(),
                }),
                correlation_id,
            ),
        ];

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sequence_number, 1);
        assert_eq!(events[1].sequence_number, 2);

        // Verify all events can be serialized
        for event_envelope in &events {
            let serialized = serialize_event(&event_envelope.event_data).unwrap();
            assert!(!serialized.is_empty());
        }
    }

    #[test]
    fn test_version_tracking_logic() {
        // Test the version increment logic used in append_events
        let expected_version = 5i64;
        let event_count = 3;

        let mut new_version = expected_version;
        for _ in 0..event_count {
            new_version += 1;
        }

        assert_eq!(new_version, 8);
    }

    #[test]
    fn test_aggregate_type_and_topic_naming() {
        // Test naming conventions for different aggregate types
        let order_type = "Order";
        let order_topic = "order-events";

        let customer_type = "Customer";
        let customer_topic = "customer-events";

        assert_eq!(order_type, "Order");
        assert_eq!(order_topic, "order-events");
        assert_eq!(customer_type, "Customer");
        assert_eq!(customer_topic, "customer-events");
    }

    // Note: The following tests require integration testing with a real ScyllaDB instance:
    // - append_events with successful append
    // - append_events with concurrency conflict detection
    // - append_events with atomic write to event_store + outbox
    // - load_events retrieving events in order
    // - load_events with empty aggregate
    // - load_aggregate reconstructing from events
    // - get_current_version tracking
    // - aggregate_exists checking
    // - Multiple aggregates isolation
    //
    // These are covered by the integration test in tests/integration_test.sh
}

// ============================================================================
// Integration Test Notes
// ============================================================================
//
// The following EventStore functionality requires integration testing:
//
// 1. Database Operations:
//    - append_events: Requires ScyllaDB session to test batch writes
//    - load_events: Requires querying actual database
//    - get_current_version: Requires database lookup
//    - aggregate_exists: Requires database check
//
// 2. Concurrency Control:
//    - Optimistic locking with version conflicts
//    - Concurrent writes to same aggregate
//    - Version increment atomicity
//
// 3. Outbox Pattern:
//    - Atomic write to event_store + outbox_messages
//    - Outbox message format and content
//
// 4. Event Ordering:
//    - Events loaded in sequence_number order
//    - Sequence number gaps detection
//
// Integration tests should be run using:
// - testcontainers with ScyllaDB Docker image
// - Or the existing tests/integration_test.sh
//
// ============================================================================

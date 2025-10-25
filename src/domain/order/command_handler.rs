use std::sync::Arc;
use uuid::Uuid;
use anyhow::{Result, bail};

use crate::event_sourcing::{AggregateRoot, EventEnvelope, EventStore};

use super::aggregate::OrderAggregate;
use super::commands::OrderCommand;
use super::events::OrderEvent;

// ============================================================================
// Order Command Handler
// ============================================================================
//
// Orchestrates: Command → Aggregate → Events → Event Store
//
// ============================================================================

pub struct OrderCommandHandler {
    event_store: Arc<EventStore<OrderEvent>>,
}

impl OrderCommandHandler {
    pub fn new(event_store: Arc<EventStore<OrderEvent>>) -> Self {
        Self { event_store }
    }

    /// Handle a command and persist resulting events
    pub async fn handle(
        &self,
        aggregate_id: Uuid,
        command: OrderCommand,
        correlation_id: Uuid,
    ) -> Result<i64> {
        // Load current aggregate state
        let exists = self.event_store.aggregate_exists(aggregate_id).await?;
        tracing::debug!("Aggregate {} exists: {}", aggregate_id, exists);

        let (aggregate, expected_version) = if exists {
            let agg = self.event_store.load_aggregate::<OrderAggregate>(aggregate_id).await?;
            let ver = agg.version();
            tracing::debug!("Loaded aggregate {} with version: {}", aggregate_id, ver);
            (agg, ver)
        } else {
            // For CreateOrder, we don't have existing aggregate
            match &command {
                OrderCommand::CreateOrder { .. } => {
                    // Create a dummy aggregate just for validation
                    let event = OrderEvent::Created(super::events::OrderCreated {
                        customer_id: Uuid::new_v4(),
                        items: vec![],
                    });
                    let agg = OrderAggregate::apply_first_event(&event)?;
                    tracing::debug!("Creating new aggregate {} with expected_version: 0", aggregate_id);
                    (agg, 0) // Expected version is 0 for new aggregates
                }
                _ => bail!("Aggregate does not exist: {}", aggregate_id),
            }
        };

        // Handle command to get events
        let domain_events = aggregate.handle_command(&command)
            .map_err(|e| anyhow::anyhow!("Command failed: {}", e))?;

        // Wrap in envelopes
        let mut envelopes = Vec::new();
        let mut seq = expected_version;

        for domain_event in domain_events {
            seq += 1;
            let event_type = match &domain_event {
                OrderEvent::Created(_) => "OrderCreated",
                OrderEvent::ItemsUpdated(_) => "OrderItemsUpdated",
                OrderEvent::Confirmed(_) => "OrderConfirmed",
                OrderEvent::Shipped(_) => "OrderShipped",
                OrderEvent::Delivered(_) => "OrderDelivered",
                OrderEvent::Cancelled(_) => "OrderCancelled",
            };

            let envelope = EventEnvelope::new(
                aggregate_id,
                seq,
                event_type.to_string(),
                domain_event,
                correlation_id,
            );

            envelopes.push(envelope);
        }

        // Append to event store
        let new_version = self.event_store.append_events(
            aggregate_id,
            expected_version,
            envelopes,
            true, // publish to outbox
        ).await?;

        Ok(new_version)
    }
}

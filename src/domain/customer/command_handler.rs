use std::sync::Arc;
use uuid::Uuid;
use anyhow::{Result, bail};

use crate::event_sourcing::core::{Aggregate, EventEnvelope};
use crate::event_sourcing::store::EventStore;

use super::aggregate::CustomerAggregate;
use super::commands::CustomerCommand;
use super::events::CustomerEvent;

// ============================================================================
// Customer Command Handler
// ============================================================================
//
// Orchestrates: Command → Aggregate → Events → Event Store
//
// ============================================================================

pub struct CustomerCommandHandler {
    event_store: Arc<EventStore<CustomerEvent>>,
}

impl CustomerCommandHandler {
    pub fn new(event_store: Arc<EventStore<CustomerEvent>>) -> Self {
        Self { event_store }
    }

    /// Handle a command and persist resulting events
    pub async fn handle(
        &self,
        aggregate_id: Uuid,
        command: CustomerCommand,
        correlation_id: Uuid,
    ) -> Result<i64> {
        // Load current aggregate state
        let (aggregate, expected_version) = if self.event_store.aggregate_exists(aggregate_id).await? {
            let agg = self.event_store.load_aggregate::<CustomerAggregate>(aggregate_id).await?;
            let ver = agg.version();
            (agg, ver)
        } else {
            // For RegisterCustomer, we don't have existing aggregate
            match &command {
                CustomerCommand::RegisterCustomer { .. } => {
                    // Create a dummy aggregate just for validation
                    let event = CustomerEvent::Registered(super::events::CustomerRegistered {
                        email: super::value_objects::Email::new("temp@example.com"),
                        first_name: String::new(),
                        last_name: String::new(),
                        phone: None,
                    });
                    let agg = CustomerAggregate::apply_first_event(&event)?;
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
                CustomerEvent::Registered(_) => "CustomerRegistered",
                CustomerEvent::ProfileUpdated(_) => "CustomerProfileUpdated",
                CustomerEvent::EmailChanged(_) => "CustomerEmailChanged",
                CustomerEvent::PhoneChanged(_) => "CustomerPhoneChanged",
                CustomerEvent::AddressAdded(_) => "CustomerAddressAdded",
                CustomerEvent::AddressUpdated(_) => "CustomerAddressUpdated",
                CustomerEvent::AddressRemoved(_) => "CustomerAddressRemoved",
                CustomerEvent::PaymentMethodAdded(_) => "CustomerPaymentMethodAdded",
                CustomerEvent::PaymentMethodRemoved(_) => "CustomerPaymentMethodRemoved",
                CustomerEvent::TierUpgraded(_) => "CustomerTierUpgraded",
                CustomerEvent::Suspended(_) => "CustomerSuspended",
                CustomerEvent::Reactivated(_) => "CustomerReactivated",
                CustomerEvent::Deactivated(_) => "CustomerDeactivated",
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

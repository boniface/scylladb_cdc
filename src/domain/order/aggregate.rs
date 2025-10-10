use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use anyhow::{Result, bail};

use crate::event_sourcing::core::{Aggregate, EventEnvelope};
use super::value_objects::{OrderItem, OrderStatus};
use super::events::*;
use super::commands::OrderCommand;
use super::errors::OrderError;

// ============================================================================
// Order Aggregate - Domain Logic
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderAggregate {
    // Identity
    pub id: Uuid,
    pub version: i64,

    // Current State (derived from events)
    pub customer_id: Uuid,
    pub items: Vec<OrderItem>,
    pub status: OrderStatus,

    // Audit Trail
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Optional fields
    pub tracking_number: Option<String>,
    pub carrier: Option<String>,
    pub cancelled_reason: Option<String>,
}

impl OrderAggregate {
    /// Load aggregate from event history
    pub fn load_from_events(events: Vec<EventEnvelope<OrderEvent>>) -> Result<Self> {
        if events.is_empty() {
            bail!("Cannot load aggregate from empty event list");
        }

        // Apply first event to create aggregate
        let first = &events[0];
        let mut aggregate = Self::apply_first_event(&first.event_data)
            .map_err(|e| anyhow::anyhow!("Failed to apply first event: {}", e))?;

        // Set version from first event
        aggregate.version = first.sequence_number;

        // Apply remaining events
        for envelope in events.iter().skip(1) {
            aggregate.apply_event(&envelope.event_data)
                .map_err(|e| anyhow::anyhow!("Failed to apply event: {}", e))?;
            aggregate.version = envelope.sequence_number;
        }

        Ok(aggregate)
    }

    /// Validate business rules before emitting events
    fn validate_items(&self, items: &[OrderItem]) -> Result<(), OrderError> {
        if items.is_empty() {
            return Err(OrderError::EmptyItems);
        }

        for item in items {
            if item.quantity <= 0 {
                return Err(OrderError::InvalidQuantity(item.quantity));
            }
        }

        Ok(())
    }
}

// ============================================================================
// Aggregate Trait Implementation
// ============================================================================

impl Aggregate for OrderAggregate {
    type Event = OrderEvent;
    type Command = OrderCommand;
    type Error = OrderError;

    fn apply_first_event(event: &Self::Event) -> Result<Self, Self::Error> {
        match event {
            OrderEvent::Created(e) => {
                let now = Utc::now();
                Ok(Self {
                    id: Uuid::new_v4(), // Will be set by event envelope
                    version: 0,
                    customer_id: e.customer_id,
                    items: e.items.clone(),
                    status: OrderStatus::Created,
                    created_at: now,
                    updated_at: now,
                    tracking_number: None,
                    carrier: None,
                    cancelled_reason: None,
                })
            }
            _ => Err(OrderError::NotInitialized),
        }
    }

    fn apply_event(&mut self, event: &Self::Event) -> Result<(), Self::Error> {
        self.updated_at = Utc::now();

        match event {
            OrderEvent::Created(_) => {
                // First event already applied
                Ok(())
            }
            OrderEvent::ItemsUpdated(e) => {
                self.items = e.items.clone();
                Ok(())
            }
            OrderEvent::Confirmed(_) => {
                self.status = OrderStatus::Confirmed;
                Ok(())
            }
            OrderEvent::Shipped(e) => {
                self.status = OrderStatus::Shipped;
                self.tracking_number = Some(e.tracking_number.clone());
                self.carrier = Some(e.carrier.clone());
                Ok(())
            }
            OrderEvent::Delivered(_) => {
                self.status = OrderStatus::Delivered;
                Ok(())
            }
            OrderEvent::Cancelled(e) => {
                self.status = OrderStatus::Cancelled;
                self.cancelled_reason = e.reason.clone();
                Ok(())
            }
        }
    }

    fn handle_command(&self, command: &Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            OrderCommand::CreateOrder { customer_id, items, .. } => {
                self.validate_items(items)?;

                Ok(vec![OrderEvent::Created(OrderCreated {
                    customer_id: *customer_id,
                    items: items.clone(),
                })])
            }

            OrderCommand::UpdateItems { items, reason } => {
                // Validate status
                match self.status {
                    OrderStatus::Cancelled => return Err(OrderError::AlreadyCancelled),
                    OrderStatus::Confirmed | OrderStatus::Shipped | OrderStatus::Delivered => {
                        return Err(OrderError::InvalidStatusTransition(self.status.clone()))
                    }
                    OrderStatus::Created => {} // OK
                }

                self.validate_items(items)?;

                Ok(vec![OrderEvent::ItemsUpdated(OrderItemsUpdated {
                    items: items.clone(),
                    reason: reason.clone(),
                })])
            }

            OrderCommand::ConfirmOrder => {
                match self.status {
                    OrderStatus::Created => {}
                    OrderStatus::Confirmed => return Err(OrderError::AlreadyConfirmed),
                    _ => return Err(OrderError::InvalidStatusTransition(self.status.clone())),
                }

                Ok(vec![OrderEvent::Confirmed(OrderConfirmed {
                    confirmed_at: Utc::now(),
                })])
            }

            OrderCommand::ShipOrder { tracking_number, carrier } => {
                match self.status {
                    OrderStatus::Confirmed => {}
                    OrderStatus::Created => return Err(OrderError::NotConfirmed),
                    _ => return Err(OrderError::InvalidStatusTransition(self.status.clone())),
                }

                Ok(vec![OrderEvent::Shipped(OrderShipped {
                    tracking_number: tracking_number.clone(),
                    carrier: carrier.clone(),
                    shipped_at: Utc::now(),
                })])
            }

            OrderCommand::DeliverOrder { signature } => {
                match self.status {
                    OrderStatus::Shipped => {}
                    _ => return Err(OrderError::NotShipped),
                }

                Ok(vec![OrderEvent::Delivered(OrderDelivered {
                    delivered_at: Utc::now(),
                    signature: signature.clone(),
                })])
            }

            OrderCommand::CancelOrder { reason, cancelled_by } => {
                match self.status {
                    OrderStatus::Cancelled => return Err(OrderError::AlreadyCancelled),
                    OrderStatus::Delivered => {
                        return Err(OrderError::InvalidStatusTransition(self.status.clone()))
                    }
                    _ => {} // Can cancel from Created, Confirmed, or Shipped
                }

                Ok(vec![OrderEvent::Cancelled(OrderCancelled {
                    reason: reason.clone(),
                    cancelled_by: *cancelled_by,
                })])
            }
        }
    }

    fn aggregate_id(&self) -> Uuid {
        self.id
    }

    fn version(&self) -> i64 {
        self.version
    }
}

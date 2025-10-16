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
    // load_from_events is now in the Aggregate trait implementation below

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

    fn load_from_events(events: Vec<EventEnvelope<Self::Event>>) -> Result<Self> {
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
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::order::commands::OrderCommand;
    use crate::event_sourcing::core::EventEnvelope;

    fn create_test_items() -> Vec<OrderItem> {
        vec![
            OrderItem { product_id: Uuid::new_v4(), quantity: 2 },
            OrderItem { product_id: Uuid::new_v4(), quantity: 1 },
        ]
    }

    #[test]
    fn test_order_creation_with_valid_items() {
        let customer_id = Uuid::new_v4();
        let items = create_test_items();

        let event = OrderEvent::Created(OrderCreated {
            customer_id,
            items: items.clone(),
        });

        let aggregate = OrderAggregate::apply_first_event(&event).unwrap();

        assert_eq!(aggregate.customer_id, customer_id);
        assert_eq!(aggregate.items.len(), 2);
        assert_eq!(aggregate.status, OrderStatus::Created);
        assert_eq!(aggregate.version, 0);
        assert!(aggregate.tracking_number.is_none());
        assert!(aggregate.carrier.is_none());
        assert!(aggregate.cancelled_reason.is_none());
    }

    #[test]
    fn test_order_creation_with_empty_items_fails() {
        let customer_id = Uuid::new_v4();
        let items: Vec<OrderItem> = vec![];

        let aggregate = OrderAggregate::apply_first_event(&OrderEvent::Created(OrderCreated {
            customer_id,
            items: vec![],
        })).unwrap();

        let command = OrderCommand::CreateOrder {
            order_id: Uuid::new_v4(),
            customer_id,
            items,
        };

        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OrderError::EmptyItems));
    }

    #[test]
    fn test_order_creation_with_invalid_quantity_fails() {
        let customer_id = Uuid::new_v4();
        let items = vec![
            OrderItem { product_id: Uuid::new_v4(), quantity: 0 },
        ];

        let aggregate = OrderAggregate::apply_first_event(&OrderEvent::Created(OrderCreated {
            customer_id,
            items: vec![OrderItem { product_id: Uuid::new_v4(), quantity: 1 }],
        })).unwrap();

        let command = OrderCommand::CreateOrder {
            order_id: Uuid::new_v4(),
            customer_id,
            items,
        };

        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OrderError::InvalidQuantity(_)));
    }

    #[test]
    fn test_order_state_transition_created_to_confirmed() {
        let customer_id = Uuid::new_v4();
        let items = create_test_items();

        let mut aggregate = OrderAggregate::apply_first_event(&OrderEvent::Created(OrderCreated {
            customer_id,
            items: items.clone(),
        })).unwrap();

        assert_eq!(aggregate.status, OrderStatus::Created);

        let confirm_event = OrderEvent::Confirmed(OrderConfirmed {
            confirmed_at: Utc::now(),
        });

        aggregate.apply_event(&confirm_event).unwrap();
        assert_eq!(aggregate.status, OrderStatus::Confirmed);
    }

    #[test]
    fn test_order_state_transition_confirmed_to_shipped() {
        let customer_id = Uuid::new_v4();
        let items = create_test_items();

        let mut aggregate = OrderAggregate::apply_first_event(&OrderEvent::Created(OrderCreated {
            customer_id,
            items: items.clone(),
        })).unwrap();

        aggregate.apply_event(&OrderEvent::Confirmed(OrderConfirmed {
            confirmed_at: Utc::now(),
        })).unwrap();

        let ship_event = OrderEvent::Shipped(OrderShipped {
            tracking_number: "TRACK123".to_string(),
            carrier: "FedEx".to_string(),
            shipped_at: Utc::now(),
        });

        aggregate.apply_event(&ship_event).unwrap();
        assert_eq!(aggregate.status, OrderStatus::Shipped);
        assert_eq!(aggregate.tracking_number, Some("TRACK123".to_string()));
        assert_eq!(aggregate.carrier, Some("FedEx".to_string()));
    }

    #[test]
    fn test_order_state_transition_shipped_to_delivered() {
        let customer_id = Uuid::new_v4();
        let items = create_test_items();

        let mut aggregate = OrderAggregate::apply_first_event(&OrderEvent::Created(OrderCreated {
            customer_id,
            items: items.clone(),
        })).unwrap();

        aggregate.apply_event(&OrderEvent::Confirmed(OrderConfirmed {
            confirmed_at: Utc::now(),
        })).unwrap();

        aggregate.apply_event(&OrderEvent::Shipped(OrderShipped {
            tracking_number: "TRACK123".to_string(),
            carrier: "FedEx".to_string(),
            shipped_at: Utc::now(),
        })).unwrap();

        let deliver_event = OrderEvent::Delivered(OrderDelivered {
            delivered_at: Utc::now(),
            signature: Some("John Doe".to_string()),
        });

        aggregate.apply_event(&deliver_event).unwrap();
        assert_eq!(aggregate.status, OrderStatus::Delivered);
    }

    #[test]
    fn test_cannot_ship_before_confirming() {
        let customer_id = Uuid::new_v4();
        let items = create_test_items();

        let aggregate = OrderAggregate::apply_first_event(&OrderEvent::Created(OrderCreated {
            customer_id,
            items: items.clone(),
        })).unwrap();

        let command = OrderCommand::ShipOrder {
            tracking_number: "TRACK123".to_string(),
            carrier: "FedEx".to_string(),
        };

        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OrderError::NotConfirmed));
    }

    #[test]
    fn test_cannot_deliver_before_shipping() {
        let customer_id = Uuid::new_v4();
        let items = create_test_items();

        let mut aggregate = OrderAggregate::apply_first_event(&OrderEvent::Created(OrderCreated {
            customer_id,
            items: items.clone(),
        })).unwrap();

        aggregate.apply_event(&OrderEvent::Confirmed(OrderConfirmed {
            confirmed_at: Utc::now(),
        })).unwrap();

        let command = OrderCommand::DeliverOrder {
            signature: Some("John Doe".to_string()),
        };

        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OrderError::NotShipped));
    }

    #[test]
    fn test_order_cancellation() {
        let customer_id = Uuid::new_v4();
        let items = create_test_items();

        let mut aggregate = OrderAggregate::apply_first_event(&OrderEvent::Created(OrderCreated {
            customer_id,
            items: items.clone(),
        })).unwrap();

        let cancel_event = OrderEvent::Cancelled(OrderCancelled {
            reason: Some("Customer request".to_string()),
            cancelled_by: Some(customer_id),
        });

        aggregate.apply_event(&cancel_event).unwrap();
        assert_eq!(aggregate.status, OrderStatus::Cancelled);
        assert_eq!(aggregate.cancelled_reason, Some("Customer request".to_string()));
    }

    #[test]
    fn test_cannot_cancel_already_cancelled_order() {
        let customer_id = Uuid::new_v4();
        let items = create_test_items();

        let mut aggregate = OrderAggregate::apply_first_event(&OrderEvent::Created(OrderCreated {
            customer_id,
            items: items.clone(),
        })).unwrap();

        aggregate.apply_event(&OrderEvent::Cancelled(OrderCancelled {
            reason: Some("First cancel".to_string()),
            cancelled_by: Some(customer_id),
        })).unwrap();

        let command = OrderCommand::CancelOrder {
            reason: Some("Second cancel".to_string()),
            cancelled_by: Some(customer_id),
        };

        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OrderError::AlreadyCancelled));
    }

    #[test]
    fn test_cannot_cancel_delivered_order() {
        let customer_id = Uuid::new_v4();
        let items = create_test_items();

        let mut aggregate = OrderAggregate::apply_first_event(&OrderEvent::Created(OrderCreated {
            customer_id,
            items: items.clone(),
        })).unwrap();

        // Transition through states
        aggregate.apply_event(&OrderEvent::Confirmed(OrderConfirmed {
            confirmed_at: Utc::now(),
        })).unwrap();

        aggregate.apply_event(&OrderEvent::Shipped(OrderShipped {
            tracking_number: "TRACK123".to_string(),
            carrier: "FedEx".to_string(),
            shipped_at: Utc::now(),
        })).unwrap();

        aggregate.apply_event(&OrderEvent::Delivered(OrderDelivered {
            delivered_at: Utc::now(),
            signature: Some("John Doe".to_string()),
        })).unwrap();

        let command = OrderCommand::CancelOrder {
            reason: Some("Too late".to_string()),
            cancelled_by: Some(customer_id),
        };

        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OrderError::InvalidStatusTransition(_)));
    }

    #[test]
    fn test_update_items_in_created_status() {
        let customer_id = Uuid::new_v4();
        let items = create_test_items();

        let aggregate = OrderAggregate::apply_first_event(&OrderEvent::Created(OrderCreated {
            customer_id,
            items: items.clone(),
        })).unwrap();

        let new_items = vec![OrderItem { product_id: Uuid::new_v4(), quantity: 3 }];

        let command = OrderCommand::UpdateItems {
            items: new_items.clone(),
            reason: Some("Customer changed mind".to_string()),
        };

        let events = aggregate.handle_command(&command).unwrap();
        assert_eq!(events.len(), 1);

        match &events[0] {
            OrderEvent::ItemsUpdated(e) => {
                assert_eq!(e.items.len(), 1);
                assert_eq!(e.items[0].quantity, 3);
            }
            _ => panic!("Expected ItemsUpdated event"),
        }
    }

    #[test]
    fn test_cannot_update_items_after_confirmation() {
        let customer_id = Uuid::new_v4();
        let items = create_test_items();

        let mut aggregate = OrderAggregate::apply_first_event(&OrderEvent::Created(OrderCreated {
            customer_id,
            items: items.clone(),
        })).unwrap();

        aggregate.apply_event(&OrderEvent::Confirmed(OrderConfirmed {
            confirmed_at: Utc::now(),
        })).unwrap();

        let new_items = vec![OrderItem { product_id: Uuid::new_v4(), quantity: 3 }];

        let command = OrderCommand::UpdateItems {
            items: new_items,
            reason: Some("Should fail".to_string()),
        };

        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OrderError::InvalidStatusTransition(_)));
    }

    #[test]
    fn test_cannot_confirm_already_confirmed_order() {
        let customer_id = Uuid::new_v4();
        let items = create_test_items();

        let mut aggregate = OrderAggregate::apply_first_event(&OrderEvent::Created(OrderCreated {
            customer_id,
            items: items.clone(),
        })).unwrap();

        aggregate.apply_event(&OrderEvent::Confirmed(OrderConfirmed {
            confirmed_at: Utc::now(),
        })).unwrap();

        let command = OrderCommand::ConfirmOrder;
        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OrderError::AlreadyConfirmed));
    }

    #[test]
    fn test_version_tracking_after_event_application() {
        let customer_id = Uuid::new_v4();
        let items = create_test_items();
        let aggregate_id = Uuid::new_v4();

        let events = vec![
            EventEnvelope::new(
                aggregate_id,
                1,
                "OrderCreated".to_string(),
                OrderEvent::Created(OrderCreated {
                    customer_id,
                    items: items.clone(),
                }),
                Uuid::new_v4(),
            ),
            EventEnvelope::new(
                aggregate_id,
                2,
                "OrderConfirmed".to_string(),
                OrderEvent::Confirmed(OrderConfirmed {
                    confirmed_at: Utc::now(),
                }),
                Uuid::new_v4(),
            ),
            EventEnvelope::new(
                aggregate_id,
                3,
                "OrderShipped".to_string(),
                OrderEvent::Shipped(OrderShipped {
                    tracking_number: "TRACK123".to_string(),
                    carrier: "FedEx".to_string(),
                    shipped_at: Utc::now(),
                }),
                Uuid::new_v4(),
            ),
        ];

        let aggregate = OrderAggregate::load_from_events(events).unwrap();
        assert_eq!(aggregate.version, 3);
        assert_eq!(aggregate.status, OrderStatus::Shipped);
    }

    #[test]
    fn test_load_from_events_empty_list_fails() {
        let events: Vec<EventEnvelope<OrderEvent>> = vec![];
        let result = OrderAggregate::load_from_events(events);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_from_events_full_lifecycle() {
        let customer_id = Uuid::new_v4();
        let items = create_test_items();
        let aggregate_id = Uuid::new_v4();

        let events = vec![
            EventEnvelope::new(
                aggregate_id,
                1,
                "OrderCreated".to_string(),
                OrderEvent::Created(OrderCreated {
                    customer_id,
                    items: items.clone(),
                }),
                Uuid::new_v4(),
            ),
            EventEnvelope::new(
                aggregate_id,
                2,
                "OrderConfirmed".to_string(),
                OrderEvent::Confirmed(OrderConfirmed {
                    confirmed_at: Utc::now(),
                }),
                Uuid::new_v4(),
            ),
            EventEnvelope::new(
                aggregate_id,
                3,
                "OrderShipped".to_string(),
                OrderEvent::Shipped(OrderShipped {
                    tracking_number: "TRACK123".to_string(),
                    carrier: "FedEx".to_string(),
                    shipped_at: Utc::now(),
                }),
                Uuid::new_v4(),
            ),
            EventEnvelope::new(
                aggregate_id,
                4,
                "OrderDelivered".to_string(),
                OrderEvent::Delivered(OrderDelivered {
                    delivered_at: Utc::now(),
                    signature: Some("John Doe".to_string()),
                }),
                Uuid::new_v4(),
            ),
        ];

        let aggregate = OrderAggregate::load_from_events(events).unwrap();
        assert_eq!(aggregate.version, 4);
        assert_eq!(aggregate.status, OrderStatus::Delivered);
        assert_eq!(aggregate.customer_id, customer_id);
        assert_eq!(aggregate.tracking_number, Some("TRACK123".to_string()));
        assert_eq!(aggregate.carrier, Some("FedEx".to_string()));
    }

    #[test]
    fn test_apply_first_event_non_created_fails() {
        let event = OrderEvent::Confirmed(OrderConfirmed {
            confirmed_at: Utc::now(),
        });

        let result = OrderAggregate::apply_first_event(&event);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OrderError::NotInitialized));
    }
}

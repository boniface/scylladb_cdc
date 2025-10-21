use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::event_sourcing::DomainEvent;
use super::value_objects::OrderItem;

// ============================================================================
// Order Events - Domain Events for Order Aggregate
// ============================================================================

/// Order Event - Union type for all order events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum OrderEvent {
    Created(OrderCreated),
    ItemsUpdated(OrderItemsUpdated),
    Confirmed(OrderConfirmed),
    Shipped(OrderShipped),
    Delivered(OrderDelivered),
    Cancelled(OrderCancelled),
}

impl DomainEvent for OrderEvent {
    fn event_type() -> &'static str { "OrderEvent" }
}

// ============================================================================
// Individual Event Types
// ============================================================================

/// Order Created - Initial event in order lifecycle
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrderCreated {
    pub customer_id: Uuid,
    pub items: Vec<OrderItem>,
}

impl DomainEvent for OrderCreated {
    fn event_type() -> &'static str { "OrderCreated" }
    fn event_version() -> i32 { 1 }
}

/// Order Items Updated - Order contents modified
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrderItemsUpdated {
    pub items: Vec<OrderItem>,
    pub reason: Option<String>,
}

impl DomainEvent for OrderItemsUpdated {
    fn event_type() -> &'static str { "OrderItemsUpdated" }
    fn event_version() -> i32 { 1 }
}

/// Order Cancelled - Order lifecycle ended
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrderCancelled {
    pub reason: Option<String>,
    pub cancelled_by: Option<Uuid>,
}

impl DomainEvent for OrderCancelled {
    fn event_type() -> &'static str { "OrderCancelled" }
    fn event_version() -> i32 { 1 }
}

/// Order Confirmed - Order accepted for fulfillment
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrderConfirmed {
    pub confirmed_at: DateTime<Utc>,
}

impl DomainEvent for OrderConfirmed {
    fn event_type() -> &'static str { "OrderConfirmed" }
    fn event_version() -> i32 { 1 }
}

/// Order Shipped - Order dispatched to customer
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrderShipped {
    pub tracking_number: String,
    pub carrier: String,
    pub shipped_at: DateTime<Utc>,
}

impl DomainEvent for OrderShipped {
    fn event_type() -> &'static str { "OrderShipped" }
    fn event_version() -> i32 { 1 }
}

/// Order Delivered - Order successfully delivered
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrderDelivered {
    pub delivered_at: DateTime<Utc>,
    pub signature: Option<String>,
}

impl DomainEvent for OrderDelivered {
    fn event_type() -> &'static str { "OrderDelivered" }
    fn event_version() -> i32 { 1 }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_created_serialization() {
        let customer_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();

        let event = OrderCreated {
            customer_id,
            items: vec![OrderItem { product_id, quantity: 2 }],
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: OrderCreated = serde_json::from_str(&json).unwrap();

        assert_eq!(event.customer_id, deserialized.customer_id);
        assert_eq!(event.items.len(), deserialized.items.len());
        assert_eq!(event.items[0].product_id, deserialized.items[0].product_id);
        assert_eq!(event.items[0].quantity, deserialized.items[0].quantity);
    }

    #[test]
    fn test_order_event_enum_serialization() {
        let customer_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();

        let event = OrderEvent::Created(OrderCreated {
            customer_id,
            items: vec![OrderItem { product_id, quantity: 1 }],
        });

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: OrderEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            OrderEvent::Created(e) => {
                assert_eq!(e.customer_id, customer_id);
                assert_eq!(e.items.len(), 1);
            }
            _ => panic!("Expected OrderCreated event"),
        }
    }

    #[test]
    fn test_order_confirmed_serialization() {
        let now = Utc::now();
        let event = OrderConfirmed { confirmed_at: now };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: OrderConfirmed = serde_json::from_str(&json).unwrap();

        assert_eq!(
            event.confirmed_at.timestamp(),
            deserialized.confirmed_at.timestamp()
        );
    }

    #[test]
    fn test_order_shipped_serialization() {
        let event = OrderShipped {
            tracking_number: "TRACK123".to_string(),
            carrier: "FedEx".to_string(),
            shipped_at: Utc::now(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: OrderShipped = serde_json::from_str(&json).unwrap();

        assert_eq!(event.tracking_number, deserialized.tracking_number);
        assert_eq!(event.carrier, deserialized.carrier);
    }

    #[test]
    fn test_order_delivered_serialization() {
        let event = OrderDelivered {
            delivered_at: Utc::now(),
            signature: Some("John Doe".to_string()),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: OrderDelivered = serde_json::from_str(&json).unwrap();

        assert_eq!(event.signature, deserialized.signature);
    }

    #[test]
    fn test_order_cancelled_serialization() {
        let cancelled_by = Uuid::new_v4();
        let event = OrderCancelled {
            reason: Some("Customer request".to_string()),
            cancelled_by: Some(cancelled_by),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: OrderCancelled = serde_json::from_str(&json).unwrap();

        assert_eq!(event.reason, deserialized.reason);
        assert_eq!(event.cancelled_by, deserialized.cancelled_by);
    }

    #[test]
    fn test_order_items_updated_serialization() {
        let product_id = Uuid::new_v4();
        let event = OrderItemsUpdated {
            items: vec![OrderItem { product_id, quantity: 5 }],
            reason: Some("Quantity adjusted".to_string()),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: OrderItemsUpdated = serde_json::from_str(&json).unwrap();

        assert_eq!(event.items.len(), deserialized.items.len());
        assert_eq!(event.reason, deserialized.reason);
    }

    #[test]
    fn test_all_order_events_serialization() {
        let customer_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();

        let events = vec![
            OrderEvent::Created(OrderCreated {
                customer_id,
                items: vec![OrderItem { product_id, quantity: 1 }],
            }),
            OrderEvent::ItemsUpdated(OrderItemsUpdated {
                items: vec![OrderItem { product_id, quantity: 2 }],
                reason: Some("Updated".to_string()),
            }),
            OrderEvent::Confirmed(OrderConfirmed {
                confirmed_at: Utc::now(),
            }),
            OrderEvent::Shipped(OrderShipped {
                tracking_number: "TRACK".to_string(),
                carrier: "UPS".to_string(),
                shipped_at: Utc::now(),
            }),
            OrderEvent::Delivered(OrderDelivered {
                delivered_at: Utc::now(),
                signature: None,
            }),
            OrderEvent::Cancelled(OrderCancelled {
                reason: None,
                cancelled_by: None,
            }),
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let _deserialized: OrderEvent = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_event_type_methods() {
        assert_eq!(OrderCreated::event_type(), "OrderCreated");
        assert_eq!(OrderConfirmed::event_type(), "OrderConfirmed");
        assert_eq!(OrderShipped::event_type(), "OrderShipped");
        assert_eq!(OrderDelivered::event_type(), "OrderDelivered");
        assert_eq!(OrderCancelled::event_type(), "OrderCancelled");
        assert_eq!(OrderItemsUpdated::event_type(), "OrderItemsUpdated");
    }

    #[test]
    fn test_event_version_methods() {
        assert_eq!(OrderCreated::event_version(), 1);
        assert_eq!(OrderConfirmed::event_version(), 1);
        assert_eq!(OrderShipped::event_version(), 1);
        assert_eq!(OrderDelivered::event_version(), 1);
        assert_eq!(OrderCancelled::event_version(), 1);
        assert_eq!(OrderItemsUpdated::event_version(), 1);
    }
}

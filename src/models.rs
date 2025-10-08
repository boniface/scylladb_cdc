use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

// ============================================================================
// Domain Models
// ============================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Order {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub items: Vec<OrderItem>,
    pub status: OrderStatus,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrderItem {
    pub product_id: Uuid,
    pub quantity: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum OrderStatus {
    Created,
    Updated,
    Cancelled,
}

// ============================================================================
// Domain Events
// These represent state changes that have occurred in the system
// ============================================================================

/// Base trait for all domain events
pub trait DomainEvent {
    fn event_type(&self) -> &str;
    fn aggregate_id(&self) -> Uuid;
    fn timestamp(&self) -> DateTime<Utc>;
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrderCreatedEvent {
    pub order_id: Uuid,
    pub customer_id: Uuid,
    pub items: Vec<OrderItem>,
    pub timestamp: DateTime<Utc>,
}

impl DomainEvent for OrderCreatedEvent {
    fn event_type(&self) -> &str {
        "OrderCreated"
    }

    fn aggregate_id(&self) -> Uuid {
        self.order_id
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrderUpdatedEvent {
    pub order_id: Uuid,
    pub items: Vec<OrderItem>,
    pub timestamp: DateTime<Utc>,
}

impl DomainEvent for OrderUpdatedEvent {
    fn event_type(&self) -> &str {
        "OrderUpdated"
    }

    fn aggregate_id(&self) -> Uuid {
        self.order_id
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrderCancelledEvent {
    pub order_id: Uuid,
    pub reason: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl DomainEvent for OrderCancelledEvent {
    fn event_type(&self) -> &str {
        "OrderCancelled"
    }

    fn aggregate_id(&self) -> Uuid {
        self.order_id
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}
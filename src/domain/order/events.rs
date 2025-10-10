use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::event_sourcing::core::DomainEvent;
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

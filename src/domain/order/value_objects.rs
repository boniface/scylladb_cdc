use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Order Value Objects
// ============================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrderItem {
    pub product_id: Uuid,
    pub quantity: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderStatus {
    Created,
    Confirmed,
    Shipped,
    Delivered,
    Cancelled,
}

use uuid::Uuid;
use super::value_objects::OrderItem;

// ============================================================================
// Order Commands - Represent user intent
// ============================================================================

#[derive(Debug, Clone)]
pub enum OrderCommand {
    CreateOrder {
        order_id: Uuid,
        customer_id: Uuid,
        items: Vec<OrderItem>,
    },
    UpdateItems {
        items: Vec<OrderItem>,
        reason: Option<String>,
    },
    ConfirmOrder,
    ShipOrder {
        tracking_number: String,
        carrier: String,
    },
    DeliverOrder {
        signature: Option<String>,
    },
    CancelOrder {
        reason: Option<String>,
        cancelled_by: Option<Uuid>,
    },
}

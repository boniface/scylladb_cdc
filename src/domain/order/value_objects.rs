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

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_item_creation() {
        let product_id = Uuid::new_v4();
        let item = OrderItem {
            product_id,
            quantity: 5,
        };

        assert_eq!(item.product_id, product_id);
        assert_eq!(item.quantity, 5);
    }

    #[test]
    fn test_order_item_serialization() {
        let item = OrderItem {
            product_id: Uuid::new_v4(),
            quantity: 3,
        };

        let json = serde_json::to_string(&item).unwrap();
        let deserialized: OrderItem = serde_json::from_str(&json).unwrap();

        assert_eq!(item.product_id, deserialized.product_id);
        assert_eq!(item.quantity, deserialized.quantity);
    }

    #[test]
    fn test_order_status_equality() {
        assert_eq!(OrderStatus::Created, OrderStatus::Created);
        assert_eq!(OrderStatus::Confirmed, OrderStatus::Confirmed);
        assert_ne!(OrderStatus::Created, OrderStatus::Confirmed);
    }

    #[test]
    fn test_order_status_serialization() {
        let status = OrderStatus::Shipped;
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: OrderStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_all_order_statuses() {
        let statuses = vec![
            OrderStatus::Created,
            OrderStatus::Confirmed,
            OrderStatus::Shipped,
            OrderStatus::Delivered,
            OrderStatus::Cancelled,
        ];

        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: OrderStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }
}

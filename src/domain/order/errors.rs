use super::value_objects::OrderStatus;

// ============================================================================
// Order Business Rule Errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    #[error("Order is already cancelled")]
    AlreadyCancelled,

    #[error("Order is already confirmed")]
    AlreadyConfirmed,

    #[error("Order must be confirmed before shipping")]
    NotConfirmed,

    #[error("Order must be shipped before delivery")]
    NotShipped,

    #[error("Cannot modify order in status: {0:?}")]
    InvalidStatusTransition(OrderStatus),

    #[error("Order items cannot be empty")]
    EmptyItems,

    #[error("Invalid item quantity: {0}")]
    InvalidQuantity(i32),

    #[error("Aggregate not initialized")]
    NotInitialized,
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = OrderError::AlreadyCancelled;
        assert_eq!(err.to_string(), "Order is already cancelled");

        let err = OrderError::AlreadyConfirmed;
        assert_eq!(err.to_string(), "Order is already confirmed");

        let err = OrderError::NotConfirmed;
        assert_eq!(err.to_string(), "Order must be confirmed before shipping");

        let err = OrderError::NotShipped;
        assert_eq!(err.to_string(), "Order must be shipped before delivery");

        let err = OrderError::EmptyItems;
        assert_eq!(err.to_string(), "Order items cannot be empty");

        let err = OrderError::InvalidQuantity(0);
        assert_eq!(err.to_string(), "Invalid item quantity: 0");

        let err = OrderError::NotInitialized;
        assert_eq!(err.to_string(), "Aggregate not initialized");
    }

    #[test]
    fn test_invalid_status_transition_error() {
        let err = OrderError::InvalidStatusTransition(OrderStatus::Confirmed);
        assert!(err.to_string().contains("Confirmed"));
    }

    #[test]
    fn test_error_debug() {
        let err = OrderError::InvalidQuantity(-5);
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("InvalidQuantity"));
    }
}

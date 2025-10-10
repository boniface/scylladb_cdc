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

use super::value_objects::CustomerStatus;

// ============================================================================
// Customer Business Rule Errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum CustomerError {
    #[error("Customer is already suspended")]
    AlreadySuspended,

    #[error("Customer is already deactivated")]
    AlreadyDeactivated,

    #[error("Customer must be active to perform this operation")]
    NotActive,

    #[error("Customer must be suspended to reactivate")]
    NotSuspended,

    #[error("Invalid customer status: {0:?}")]
    InvalidStatus(CustomerStatus),

    #[error("Email cannot be empty")]
    EmptyEmail,

    #[error("Invalid email format: {0}")]
    InvalidEmail(String),

    #[error("First name cannot be empty")]
    EmptyFirstName,

    #[error("Last name cannot be empty")]
    EmptyLastName,

    #[error("Address not found: {0}")]
    AddressNotFound(uuid::Uuid),

    #[error("Payment method not found: {0}")]
    PaymentMethodNotFound(uuid::Uuid),

    #[error("Cannot remove default payment method")]
    CannotRemoveDefaultPaymentMethod,

    #[error("Customer tier cannot be downgraded")]
    TierDowngradeNotAllowed,

    #[error("Aggregate not initialized")]
    NotInitialized,
}

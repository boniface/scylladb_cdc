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

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_error_display() {
        let err = CustomerError::AlreadySuspended;
        assert_eq!(err.to_string(), "Customer is already suspended");

        let err = CustomerError::AlreadyDeactivated;
        assert_eq!(err.to_string(), "Customer is already deactivated");

        let err = CustomerError::NotActive;
        assert_eq!(err.to_string(), "Customer must be active to perform this operation");

        let err = CustomerError::NotSuspended;
        assert_eq!(err.to_string(), "Customer must be suspended to reactivate");

        let err = CustomerError::EmptyEmail;
        assert_eq!(err.to_string(), "Email cannot be empty");

        let err = CustomerError::EmptyFirstName;
        assert_eq!(err.to_string(), "First name cannot be empty");

        let err = CustomerError::EmptyLastName;
        assert_eq!(err.to_string(), "Last name cannot be empty");

        let err = CustomerError::TierDowngradeNotAllowed;
        assert_eq!(err.to_string(), "Customer tier cannot be downgraded");

        let err = CustomerError::CannotRemoveDefaultPaymentMethod;
        assert_eq!(err.to_string(), "Cannot remove default payment method");

        let err = CustomerError::NotInitialized;
        assert_eq!(err.to_string(), "Aggregate not initialized");
    }

    #[test]
    fn test_invalid_email_error() {
        let err = CustomerError::InvalidEmail("bad-email".to_string());
        assert!(err.to_string().contains("bad-email"));
        assert!(err.to_string().contains("Invalid email format"));
    }

    #[test]
    fn test_address_not_found_error() {
        let id = Uuid::new_v4();
        let err = CustomerError::AddressNotFound(id);
        assert!(err.to_string().contains(&id.to_string()));
    }

    #[test]
    fn test_payment_method_not_found_error() {
        let id = Uuid::new_v4();
        let err = CustomerError::PaymentMethodNotFound(id);
        assert!(err.to_string().contains(&id.to_string()));
    }

    #[test]
    fn test_invalid_status_error() {
        let err = CustomerError::InvalidStatus(CustomerStatus::Suspended);
        assert!(err.to_string().contains("Suspended"));
    }

    #[test]
    fn test_error_debug() {
        let err = CustomerError::EmptyEmail;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("EmptyEmail"));
    }
}

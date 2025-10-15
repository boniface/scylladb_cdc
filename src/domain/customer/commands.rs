use uuid::Uuid;
use super::value_objects::{Email, PhoneNumber, Address, CustomerTier, PaymentMethod};

// ============================================================================
// Customer Domain Commands
// ============================================================================

#[derive(Debug, Clone)]
pub enum CustomerCommand {
    RegisterCustomer {
        customer_id: Uuid,
        email: Email,
        first_name: String,
        last_name: String,
        phone: Option<PhoneNumber>,
    },
    UpdateProfile {
        first_name: Option<String>,
        last_name: Option<String>,
        phone: Option<PhoneNumber>,
    },
    ChangeEmail {
        new_email: Email,
    },
    ChangePhone {
        new_phone: PhoneNumber,
    },
    AddAddress {
        address_id: Uuid,
        address: Address,
        set_as_default: bool,
    },
    UpdateAddress {
        address_id: Uuid,
        address: Address,
    },
    RemoveAddress {
        address_id: Uuid,
    },
    AddPaymentMethod {
        payment_method: PaymentMethod,
    },
    RemovePaymentMethod {
        payment_method_id: Uuid,
    },
    UpgradeTier {
        new_tier: CustomerTier,
    },
    SuspendCustomer {
        reason: String,
    },
    ReactivateCustomer {
        notes: Option<String>,
    },
    DeactivateCustomer {
        reason: String,
    },
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::event_sourcing::core::DomainEvent;
use super::value_objects::{Email, PhoneNumber, Address, CustomerStatus, CustomerTier, PaymentMethod};

// ============================================================================
// Customer Domain Events
// ============================================================================

/// Union type for all customer events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum CustomerEvent {
    Registered(CustomerRegistered),
    ProfileUpdated(CustomerProfileUpdated),
    EmailChanged(CustomerEmailChanged),
    PhoneChanged(CustomerPhoneChanged),
    AddressAdded(CustomerAddressAdded),
    AddressUpdated(CustomerAddressUpdated),
    AddressRemoved(CustomerAddressRemoved),
    PaymentMethodAdded(CustomerPaymentMethodAdded),
    PaymentMethodRemoved(CustomerPaymentMethodRemoved),
    TierUpgraded(CustomerTierUpgraded),
    Suspended(CustomerSuspended),
    Reactivated(CustomerReactivated),
    Deactivated(CustomerDeactivated),
}

impl DomainEvent for CustomerEvent {
    fn event_type() -> &'static str {
        "CustomerEvent"
    }
}

// Individual event types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerRegistered {
    pub email: Email,
    pub first_name: String,
    pub last_name: String,
    pub phone: Option<PhoneNumber>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerProfileUpdated {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub phone: Option<PhoneNumber>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerEmailChanged {
    pub old_email: Email,
    pub new_email: Email,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerPhoneChanged {
    pub old_phone: Option<PhoneNumber>,
    pub new_phone: PhoneNumber,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerAddressAdded {
    pub address_id: Uuid,
    pub address: Address,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerAddressUpdated {
    pub address_id: Uuid,
    pub address: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerAddressRemoved {
    pub address_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerPaymentMethodAdded {
    pub payment_method: PaymentMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerPaymentMethodRemoved {
    pub payment_method_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerTierUpgraded {
    pub old_tier: CustomerTier,
    pub new_tier: CustomerTier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerSuspended {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerReactivated {
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerDeactivated {
    pub reason: String,
}

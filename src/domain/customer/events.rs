use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::event_sourcing::DomainEvent;
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

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::customer::value_objects::PaymentMethodType;

    fn create_test_address() -> Address {
        Address {
            street: "123 Main St".to_string(),
            city: "Anytown".to_string(),
            state: "CA".to_string(),
            postal_code: "12345".to_string(),
            country: "USA".to_string(),
        }
    }

    #[test]
    fn test_customer_registered_serialization() {
        let event = CustomerRegistered {
            email: Email::new("test@example.com"),
            first_name: "John".to_string(),
            last_name: "Doe".to_string(),
            phone: Some(PhoneNumber::new("555-1234")),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CustomerRegistered = serde_json::from_str(&json).unwrap();

        assert_eq!(event.email, deserialized.email);
        assert_eq!(event.first_name, deserialized.first_name);
        assert_eq!(event.last_name, deserialized.last_name);
        assert_eq!(event.phone, deserialized.phone);
    }

    #[test]
    fn test_customer_profile_updated_serialization() {
        let event = CustomerProfileUpdated {
            first_name: Some("Jane".to_string()),
            last_name: None,
            phone: Some(PhoneNumber::new("555-9999")),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CustomerProfileUpdated = serde_json::from_str(&json).unwrap();

        assert_eq!(event.first_name, deserialized.first_name);
        assert_eq!(event.last_name, deserialized.last_name);
        assert_eq!(event.phone, deserialized.phone);
    }

    #[test]
    fn test_customer_email_changed_serialization() {
        let event = CustomerEmailChanged {
            old_email: Email::new("old@example.com"),
            new_email: Email::new("new@example.com"),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CustomerEmailChanged = serde_json::from_str(&json).unwrap();

        assert_eq!(event.old_email, deserialized.old_email);
        assert_eq!(event.new_email, deserialized.new_email);
    }

    #[test]
    fn test_customer_phone_changed_serialization() {
        let event = CustomerPhoneChanged {
            old_phone: Some(PhoneNumber::new("555-1234")),
            new_phone: PhoneNumber::new("555-9999"),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CustomerPhoneChanged = serde_json::from_str(&json).unwrap();

        assert_eq!(event.old_phone, deserialized.old_phone);
        assert_eq!(event.new_phone, deserialized.new_phone);
    }

    #[test]
    fn test_customer_address_added_serialization() {
        let address_id = Uuid::new_v4();
        let event = CustomerAddressAdded {
            address_id,
            address: create_test_address(),
            is_default: true,
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CustomerAddressAdded = serde_json::from_str(&json).unwrap();

        assert_eq!(event.address_id, deserialized.address_id);
        assert_eq!(event.address, deserialized.address);
        assert_eq!(event.is_default, deserialized.is_default);
    }

    #[test]
    fn test_customer_address_updated_serialization() {
        let address_id = Uuid::new_v4();
        let event = CustomerAddressUpdated {
            address_id,
            address: create_test_address(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CustomerAddressUpdated = serde_json::from_str(&json).unwrap();

        assert_eq!(event.address_id, deserialized.address_id);
        assert_eq!(event.address, deserialized.address);
    }

    #[test]
    fn test_customer_address_removed_serialization() {
        let address_id = Uuid::new_v4();
        let event = CustomerAddressRemoved { address_id };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CustomerAddressRemoved = serde_json::from_str(&json).unwrap();

        assert_eq!(event.address_id, deserialized.address_id);
    }

    #[test]
    fn test_customer_payment_method_added_serialization() {
        let payment_method = PaymentMethod {
            id: Uuid::new_v4(),
            method_type: PaymentMethodType::CreditCard,
            last_four: "1234".to_string(),
            is_default: true,
        };

        let event = CustomerPaymentMethodAdded {
            payment_method: payment_method.clone(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CustomerPaymentMethodAdded = serde_json::from_str(&json).unwrap();

        assert_eq!(event.payment_method, deserialized.payment_method);
    }

    #[test]
    fn test_customer_payment_method_removed_serialization() {
        let payment_id = Uuid::new_v4();
        let event = CustomerPaymentMethodRemoved {
            payment_method_id: payment_id,
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CustomerPaymentMethodRemoved = serde_json::from_str(&json).unwrap();

        assert_eq!(event.payment_method_id, deserialized.payment_method_id);
    }

    #[test]
    fn test_customer_tier_upgraded_serialization() {
        let event = CustomerTierUpgraded {
            old_tier: CustomerTier::Bronze,
            new_tier: CustomerTier::Silver,
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CustomerTierUpgraded = serde_json::from_str(&json).unwrap();

        assert_eq!(event.old_tier, deserialized.old_tier);
        assert_eq!(event.new_tier, deserialized.new_tier);
    }

    #[test]
    fn test_customer_suspended_serialization() {
        let event = CustomerSuspended {
            reason: "Payment overdue".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CustomerSuspended = serde_json::from_str(&json).unwrap();

        assert_eq!(event.reason, deserialized.reason);
    }

    #[test]
    fn test_customer_reactivated_serialization() {
        let event = CustomerReactivated {
            notes: Some("Payment received".to_string()),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CustomerReactivated = serde_json::from_str(&json).unwrap();

        assert_eq!(event.notes, deserialized.notes);
    }

    #[test]
    fn test_customer_deactivated_serialization() {
        let event = CustomerDeactivated {
            reason: "Account closure".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CustomerDeactivated = serde_json::from_str(&json).unwrap();

        assert_eq!(event.reason, deserialized.reason);
    }

    #[test]
    fn test_all_customer_events_serialization() {
        let address_id = Uuid::new_v4();
        let payment_id = Uuid::new_v4();

        let events = vec![
            CustomerEvent::Registered(CustomerRegistered {
                email: Email::new("test@example.com"),
                first_name: "John".to_string(),
                last_name: "Doe".to_string(),
                phone: None,
            }),
            CustomerEvent::ProfileUpdated(CustomerProfileUpdated {
                first_name: Some("Jane".to_string()),
                last_name: None,
                phone: None,
            }),
            CustomerEvent::EmailChanged(CustomerEmailChanged {
                old_email: Email::new("old@example.com"),
                new_email: Email::new("new@example.com"),
            }),
            CustomerEvent::PhoneChanged(CustomerPhoneChanged {
                old_phone: None,
                new_phone: PhoneNumber::new("555-1234"),
            }),
            CustomerEvent::AddressAdded(CustomerAddressAdded {
                address_id,
                address: create_test_address(),
                is_default: true,
            }),
            CustomerEvent::AddressUpdated(CustomerAddressUpdated {
                address_id,
                address: create_test_address(),
            }),
            CustomerEvent::AddressRemoved(CustomerAddressRemoved { address_id }),
            CustomerEvent::PaymentMethodAdded(CustomerPaymentMethodAdded {
                payment_method: PaymentMethod {
                    id: payment_id,
                    method_type: PaymentMethodType::CreditCard,
                    last_four: "1234".to_string(),
                    is_default: false,
                },
            }),
            CustomerEvent::PaymentMethodRemoved(CustomerPaymentMethodRemoved {
                payment_method_id: payment_id,
            }),
            CustomerEvent::TierUpgraded(CustomerTierUpgraded {
                old_tier: CustomerTier::Bronze,
                new_tier: CustomerTier::Silver,
            }),
            CustomerEvent::Suspended(CustomerSuspended {
                reason: "Test".to_string(),
            }),
            CustomerEvent::Reactivated(CustomerReactivated { notes: None }),
            CustomerEvent::Deactivated(CustomerDeactivated {
                reason: "Test".to_string(),
            }),
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let _deserialized: CustomerEvent = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_customer_event_enum_variants() {
        let event = CustomerEvent::Registered(CustomerRegistered {
            email: Email::new("test@example.com"),
            first_name: "John".to_string(),
            last_name: "Doe".to_string(),
            phone: None,
        });

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CustomerEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            CustomerEvent::Registered(e) => {
                assert_eq!(e.email.as_str(), "test@example.com");
                assert_eq!(e.first_name, "John");
            }
            _ => panic!("Expected Registered event"),
        }
    }
}

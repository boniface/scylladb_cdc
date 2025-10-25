use uuid::Uuid;
use std::collections::HashMap;
use anyhow::Result;

use crate::event_sourcing::{AggregateRoot, EventEnvelope};
use super::value_objects::{Email, PhoneNumber, Address, CustomerStatus, CustomerTier, PaymentMethod};
use super::commands::CustomerCommand;
use super::events::*;
use super::errors::CustomerError;

// ============================================================================
// Customer Aggregate - Business Logic
// ============================================================================

#[derive(Debug, Clone)]
pub struct CustomerAggregate {
    pub customer_id: Uuid,
    pub version: i64,
    pub email: Email,
    pub first_name: String,
    pub last_name: String,
    pub phone: Option<PhoneNumber>,
    pub status: CustomerStatus,
    pub tier: CustomerTier,
    pub addresses: HashMap<Uuid, Address>,
    pub default_address_id: Option<Uuid>,
    pub payment_methods: HashMap<Uuid, PaymentMethod>,
}

impl CustomerAggregate {
    // load_from_events is now in the Aggregate trait implementation below

    /// Validate email format (basic validation)
    fn validate_email(&self, email: &Email) -> Result<(), CustomerError> {
        if email.as_str().is_empty() {
            return Err(CustomerError::EmptyEmail);
        }
        if !email.as_str().contains('@') {
            return Err(CustomerError::InvalidEmail(email.as_str().to_string()));
        }
        Ok(())
    }

    /// Validate customer can be modified
    fn validate_active(&self) -> Result<(), CustomerError> {
        match self.status {
            CustomerStatus::Active => Ok(()),
            _ => Err(CustomerError::NotActive),
        }
    }
}

impl AggregateRoot for CustomerAggregate {
    type Event = CustomerEvent;
    type Command = CustomerCommand;
    type Error = CustomerError;

    fn apply_first_event(event: &Self::Event) -> Result<Self, Self::Error> {
        match event {
            CustomerEvent::Registered(e) => {
                Ok(Self {
                    customer_id: Uuid::new_v4(), // Will be overridden by actual ID
                    version: 0,
                    email: e.email.clone(),
                    first_name: e.first_name.clone(),
                    last_name: e.last_name.clone(),
                    phone: e.phone.clone(),
                    status: CustomerStatus::Active,
                    tier: CustomerTier::Bronze,
                    addresses: HashMap::new(),
                    default_address_id: None,
                    payment_methods: HashMap::new(),
                })
            }
            _ => Err(CustomerError::NotInitialized),
        }
    }

    fn apply_event(&mut self, event: &Self::Event) -> Result<(), Self::Error> {
        match event {
            CustomerEvent::Registered(_) => {
                // Already applied in apply_first_event
            }
            CustomerEvent::ProfileUpdated(e) => {
                if let Some(ref first_name) = e.first_name {
                    self.first_name = first_name.clone();
                }
                if let Some(ref last_name) = e.last_name {
                    self.last_name = last_name.clone();
                }
                if let Some(ref phone) = e.phone {
                    self.phone = Some(phone.clone());
                }
            }
            CustomerEvent::EmailChanged(e) => {
                self.email = e.new_email.clone();
            }
            CustomerEvent::PhoneChanged(e) => {
                self.phone = Some(e.new_phone.clone());
            }
            CustomerEvent::AddressAdded(e) => {
                self.addresses.insert(e.address_id, e.address.clone());
                if e.is_default {
                    self.default_address_id = Some(e.address_id);
                }
            }
            CustomerEvent::AddressUpdated(e) => {
                self.addresses.insert(e.address_id, e.address.clone());
            }
            CustomerEvent::AddressRemoved(e) => {
                self.addresses.remove(&e.address_id);
                if self.default_address_id == Some(e.address_id) {
                    self.default_address_id = None;
                }
            }
            CustomerEvent::PaymentMethodAdded(e) => {
                self.payment_methods.insert(e.payment_method.id, e.payment_method.clone());
            }
            CustomerEvent::PaymentMethodRemoved(e) => {
                self.payment_methods.remove(&e.payment_method_id);
            }
            CustomerEvent::TierUpgraded(e) => {
                self.tier = e.new_tier.clone();
            }
            CustomerEvent::Suspended(_) => {
                self.status = CustomerStatus::Suspended;
            }
            CustomerEvent::Reactivated(_) => {
                self.status = CustomerStatus::Active;
            }
            CustomerEvent::Deactivated(_) => {
                self.status = CustomerStatus::Deactivated;
            }
        }

        self.version += 1;
        Ok(())
    }

    fn handle_command(&self, command: &Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            CustomerCommand::RegisterCustomer { email, first_name, last_name, phone, .. } => {
                // Validate
                if first_name.is_empty() {
                    return Err(CustomerError::EmptyFirstName);
                }
                if last_name.is_empty() {
                    return Err(CustomerError::EmptyLastName);
                }
                self.validate_email(email)?;

                Ok(vec![CustomerEvent::Registered(CustomerRegistered {
                    email: email.clone(),
                    first_name: first_name.clone(),
                    last_name: last_name.clone(),
                    phone: phone.clone(),
                })])
            }

            CustomerCommand::UpdateProfile { first_name, last_name, phone } => {
                self.validate_active()?;

                Ok(vec![CustomerEvent::ProfileUpdated(CustomerProfileUpdated {
                    first_name: first_name.clone(),
                    last_name: last_name.clone(),
                    phone: phone.clone(),
                })])
            }

            CustomerCommand::ChangeEmail { new_email } => {
                self.validate_active()?;
                self.validate_email(new_email)?;

                if &self.email == new_email {
                    return Ok(vec![]); // No change
                }

                Ok(vec![CustomerEvent::EmailChanged(CustomerEmailChanged {
                    old_email: self.email.clone(),
                    new_email: new_email.clone(),
                })])
            }

            CustomerCommand::ChangePhone { new_phone } => {
                self.validate_active()?;

                Ok(vec![CustomerEvent::PhoneChanged(CustomerPhoneChanged {
                    old_phone: self.phone.clone(),
                    new_phone: new_phone.clone(),
                })])
            }

            CustomerCommand::AddAddress { address_id, address, set_as_default } => {
                self.validate_active()?;

                Ok(vec![CustomerEvent::AddressAdded(CustomerAddressAdded {
                    address_id: *address_id,
                    address: address.clone(),
                    is_default: *set_as_default,
                })])
            }

            CustomerCommand::UpdateAddress { address_id, address } => {
                self.validate_active()?;

                if !self.addresses.contains_key(address_id) {
                    return Err(CustomerError::AddressNotFound(*address_id));
                }

                Ok(vec![CustomerEvent::AddressUpdated(CustomerAddressUpdated {
                    address_id: *address_id,
                    address: address.clone(),
                })])
            }

            CustomerCommand::RemoveAddress { address_id } => {
                self.validate_active()?;

                if !self.addresses.contains_key(address_id) {
                    return Err(CustomerError::AddressNotFound(*address_id));
                }

                Ok(vec![CustomerEvent::AddressRemoved(CustomerAddressRemoved {
                    address_id: *address_id,
                })])
            }

            CustomerCommand::AddPaymentMethod { payment_method } => {
                self.validate_active()?;

                Ok(vec![CustomerEvent::PaymentMethodAdded(CustomerPaymentMethodAdded {
                    payment_method: payment_method.clone(),
                })])
            }

            CustomerCommand::RemovePaymentMethod { payment_method_id } => {
                self.validate_active()?;

                if !self.payment_methods.contains_key(payment_method_id) {
                    return Err(CustomerError::PaymentMethodNotFound(*payment_method_id));
                }

                Ok(vec![CustomerEvent::PaymentMethodRemoved(CustomerPaymentMethodRemoved {
                    payment_method_id: *payment_method_id,
                })])
            }

            CustomerCommand::UpgradeTier { new_tier } => {
                self.validate_active()?;

                // Business rule: can only upgrade, not downgrade
                let current_level = match self.tier {
                    CustomerTier::Bronze => 0,
                    CustomerTier::Silver => 1,
                    CustomerTier::Gold => 2,
                    CustomerTier::Platinum => 3,
                };

                let new_level = match new_tier {
                    CustomerTier::Bronze => 0,
                    CustomerTier::Silver => 1,
                    CustomerTier::Gold => 2,
                    CustomerTier::Platinum => 3,
                };

                if new_level <= current_level {
                    return Err(CustomerError::TierDowngradeNotAllowed);
                }

                Ok(vec![CustomerEvent::TierUpgraded(CustomerTierUpgraded {
                    old_tier: self.tier.clone(),
                    new_tier: new_tier.clone(),
                })])
            }

            CustomerCommand::SuspendCustomer { reason } => {
                if self.status == CustomerStatus::Suspended {
                    return Err(CustomerError::AlreadySuspended);
                }
                if self.status == CustomerStatus::Deactivated {
                    return Err(CustomerError::AlreadyDeactivated);
                }

                Ok(vec![CustomerEvent::Suspended(CustomerSuspended {
                    reason: reason.clone(),
                })])
            }

            CustomerCommand::ReactivateCustomer { notes } => {
                if self.status != CustomerStatus::Suspended {
                    return Err(CustomerError::NotSuspended);
                }

                Ok(vec![CustomerEvent::Reactivated(CustomerReactivated {
                    notes: notes.clone(),
                })])
            }

            CustomerCommand::DeactivateCustomer { reason } => {
                if self.status == CustomerStatus::Deactivated {
                    return Err(CustomerError::AlreadyDeactivated);
                }

                Ok(vec![CustomerEvent::Deactivated(CustomerDeactivated {
                    reason: reason.clone(),
                })])
            }
        }
    }

    fn aggregate_id(&self) -> Uuid {
        self.customer_id
    }

    fn version(&self) -> i64 {
        self.version
    }

    fn load_from_events(events: Vec<EventEnvelope<Self::Event>>) -> Result<Self> {
        if events.is_empty() {
            anyhow::bail!("No events to load");
        }

        // Apply first event to create aggregate
        let mut aggregate = Self::apply_first_event(&events[0].event_data)
            .map_err(|e| anyhow::anyhow!("Failed to apply first event: {}", e))?;

        // Set version from first event
        aggregate.version = events[0].sequence_number;

        // Apply remaining events
        for envelope in events.iter().skip(1) {
            aggregate.apply_event(&envelope.event_data)
                .map_err(|e| anyhow::anyhow!("Failed to apply event: {}", e))?;
            aggregate.version = envelope.sequence_number;
        }

        Ok(aggregate)
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::customer::value_objects::{PaymentMethodType};

    fn create_test_customer() -> CustomerRegistered {
        CustomerRegistered {
            email: Email::new("test@example.com"),
            first_name: "John".to_string(),
            last_name: "Doe".to_string(),
            phone: Some(PhoneNumber::new("555-1234")),
        }
    }

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
    fn test_customer_registration() {
        let event = CustomerEvent::Registered(create_test_customer());
        let aggregate = CustomerAggregate::apply_first_event(&event).unwrap();

        assert_eq!(aggregate.email.as_str(), "test@example.com");
        assert_eq!(aggregate.first_name, "John");
        assert_eq!(aggregate.last_name, "Doe");
        assert_eq!(aggregate.status, CustomerStatus::Active);
        assert_eq!(aggregate.tier, CustomerTier::Bronze);
        assert!(aggregate.addresses.is_empty());
        assert!(aggregate.payment_methods.is_empty());
    }

    #[test]
    fn test_customer_registration_with_empty_name_fails() {
        let email = Email::new("test@example.com");

        let aggregate = CustomerAggregate::apply_first_event(&CustomerEvent::Registered(create_test_customer())).unwrap();

        let command = CustomerCommand::RegisterCustomer {
            customer_id: Uuid::new_v4(),
            email,
            first_name: "".to_string(),
            last_name: "Doe".to_string(),
            phone: None,
        };

        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CustomerError::EmptyFirstName));
    }

    #[test]
    fn test_customer_registration_with_invalid_email_fails() {
        let aggregate = CustomerAggregate::apply_first_event(&CustomerEvent::Registered(create_test_customer())).unwrap();

        let command = CustomerCommand::RegisterCustomer {
            customer_id: Uuid::new_v4(),
            email: Email::new("invalid-email"),
            first_name: "John".to_string(),
            last_name: "Doe".to_string(),
            phone: None,
        };

        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CustomerError::InvalidEmail(_)));
    }

    #[test]
    fn test_profile_update() {
        let mut aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        let event = CustomerEvent::ProfileUpdated(CustomerProfileUpdated {
            first_name: Some("Jane".to_string()),
            last_name: None,
            phone: Some(PhoneNumber::new("555-9999")),
        });

        aggregate.apply_event(&event).unwrap();
        assert_eq!(aggregate.first_name, "Jane");
        assert_eq!(aggregate.last_name, "Doe"); // unchanged
        assert_eq!(aggregate.phone.as_ref().unwrap().as_str(), "555-9999");
    }

    #[test]
    fn test_email_change() {
        let mut aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        let new_email = Email::new("newemail@example.com");
        let event = CustomerEvent::EmailChanged(CustomerEmailChanged {
            old_email: aggregate.email.clone(),
            new_email: new_email.clone(),
        });

        aggregate.apply_event(&event).unwrap();
        assert_eq!(aggregate.email, new_email);
    }

    #[test]
    fn test_add_address() {
        let mut aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        let address_id = Uuid::new_v4();
        let address = create_test_address();

        let event = CustomerEvent::AddressAdded(CustomerAddressAdded {
            address_id,
            address: address.clone(),
            is_default: true,
        });

        aggregate.apply_event(&event).unwrap();
        assert_eq!(aggregate.addresses.len(), 1);
        assert!(aggregate.addresses.contains_key(&address_id));
        assert_eq!(aggregate.default_address_id, Some(address_id));
    }

    #[test]
    fn test_update_address() {
        let mut aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        let address_id = Uuid::new_v4();
        let address = create_test_address();

        aggregate.apply_event(&CustomerEvent::AddressAdded(CustomerAddressAdded {
            address_id,
            address: address.clone(),
            is_default: false,
        })).unwrap();

        let mut updated_address = create_test_address();
        updated_address.street = "456 Oak Ave".to_string();

        let command = CustomerCommand::UpdateAddress {
            address_id,
            address: updated_address.clone(),
        };

        let events = aggregate.handle_command(&command).unwrap();
        assert_eq!(events.len(), 1);

        aggregate.apply_event(&events[0]).unwrap();
        assert_eq!(aggregate.addresses.get(&address_id).unwrap().street, "456 Oak Ave");
    }

    #[test]
    fn test_remove_address() {
        let mut aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        let address_id = Uuid::new_v4();
        let address = create_test_address();

        aggregate.apply_event(&CustomerEvent::AddressAdded(CustomerAddressAdded {
            address_id,
            address,
            is_default: true,
        })).unwrap();

        let event = CustomerEvent::AddressRemoved(CustomerAddressRemoved { address_id });
        aggregate.apply_event(&event).unwrap();

        assert_eq!(aggregate.addresses.len(), 0);
        assert_eq!(aggregate.default_address_id, None);
    }

    #[test]
    fn test_add_payment_method() {
        let mut aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        let payment_method = PaymentMethod {
            id: Uuid::new_v4(),
            method_type: PaymentMethodType::CreditCard,
            last_four: "1234".to_string(),
            is_default: true,
        };

        let event = CustomerEvent::PaymentMethodAdded(CustomerPaymentMethodAdded {
            payment_method: payment_method.clone(),
        });

        aggregate.apply_event(&event).unwrap();
        assert_eq!(aggregate.payment_methods.len(), 1);
        assert!(aggregate.payment_methods.contains_key(&payment_method.id));
    }

    #[test]
    fn test_remove_payment_method() {
        let mut aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        let payment_id = Uuid::new_v4();
        let payment_method = PaymentMethod {
            id: payment_id,
            method_type: PaymentMethodType::CreditCard,
            last_four: "1234".to_string(),
            is_default: true,
        };

        aggregate.apply_event(&CustomerEvent::PaymentMethodAdded(
            CustomerPaymentMethodAdded { payment_method }
        )).unwrap();

        let event = CustomerEvent::PaymentMethodRemoved(CustomerPaymentMethodRemoved {
            payment_method_id: payment_id,
        });

        aggregate.apply_event(&event).unwrap();
        assert_eq!(aggregate.payment_methods.len(), 0);
    }

    #[test]
    fn test_tier_upgrade() {
        let mut aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        assert_eq!(aggregate.tier, CustomerTier::Bronze);

        let event = CustomerEvent::TierUpgraded(CustomerTierUpgraded {
            old_tier: CustomerTier::Bronze,
            new_tier: CustomerTier::Silver,
        });

        aggregate.apply_event(&event).unwrap();
        assert_eq!(aggregate.tier, CustomerTier::Silver);
    }

    #[test]
    fn test_tier_downgrade_not_allowed() {
        let mut aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        aggregate.apply_event(&CustomerEvent::TierUpgraded(CustomerTierUpgraded {
            old_tier: CustomerTier::Bronze,
            new_tier: CustomerTier::Gold,
        })).unwrap();

        let command = CustomerCommand::UpgradeTier {
            new_tier: CustomerTier::Silver,
        };

        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CustomerError::TierDowngradeNotAllowed));
    }

    #[test]
    fn test_customer_suspension() {
        let mut aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        assert_eq!(aggregate.status, CustomerStatus::Active);

        let event = CustomerEvent::Suspended(CustomerSuspended {
            reason: "Payment overdue".to_string(),
        });

        aggregate.apply_event(&event).unwrap();
        assert_eq!(aggregate.status, CustomerStatus::Suspended);
    }

    #[test]
    fn test_customer_reactivation() {
        let mut aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        aggregate.apply_event(&CustomerEvent::Suspended(CustomerSuspended {
            reason: "Test".to_string(),
        })).unwrap();

        let event = CustomerEvent::Reactivated(CustomerReactivated {
            notes: Some("Payment received".to_string()),
        });

        aggregate.apply_event(&event).unwrap();
        assert_eq!(aggregate.status, CustomerStatus::Active);
    }

    #[test]
    fn test_customer_deactivation() {
        let mut aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        let event = CustomerEvent::Deactivated(CustomerDeactivated {
            reason: "Account closure requested".to_string(),
        });

        aggregate.apply_event(&event).unwrap();
        assert_eq!(aggregate.status, CustomerStatus::Deactivated);
    }

    #[test]
    fn test_cannot_modify_suspended_customer() {
        let mut aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        aggregate.apply_event(&CustomerEvent::Suspended(CustomerSuspended {
            reason: "Test".to_string(),
        })).unwrap();

        let command = CustomerCommand::UpdateProfile {
            first_name: Some("Jane".to_string()),
            last_name: None,
            phone: None,
        };

        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CustomerError::NotActive));
    }

    #[test]
    fn test_cannot_reactivate_active_customer() {
        let aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        let command = CustomerCommand::ReactivateCustomer {
            notes: Some("Test".to_string()),
        };

        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CustomerError::NotSuspended));
    }

    #[test]
    fn test_cannot_update_nonexistent_address() {
        let aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        let address_id = Uuid::new_v4();
        let command = CustomerCommand::UpdateAddress {
            address_id,
            address: create_test_address(),
        };

        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CustomerError::AddressNotFound(_)));
    }

    #[test]
    fn test_cannot_remove_nonexistent_payment_method() {
        let aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        let payment_id = Uuid::new_v4();
        let command = CustomerCommand::RemovePaymentMethod {
            payment_method_id: payment_id,
        };

        let result = aggregate.handle_command(&command);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CustomerError::PaymentMethodNotFound(_)));
    }

    #[test]
    fn test_load_from_events_full_lifecycle() {
        let customer_id = Uuid::new_v4();
        let address_id = Uuid::new_v4();
        let payment_id = Uuid::new_v4();

        let events = vec![
            EventEnvelope::new(
                customer_id,
                1,
                "CustomerRegistered".to_string(),
                CustomerEvent::Registered(create_test_customer()),
                Uuid::new_v4(),
            ),
            EventEnvelope::new(
                customer_id,
                2,
                "AddressAdded".to_string(),
                CustomerEvent::AddressAdded(CustomerAddressAdded {
                    address_id,
                    address: create_test_address(),
                    is_default: true,
                }),
                Uuid::new_v4(),
            ),
            EventEnvelope::new(
                customer_id,
                3,
                "PaymentMethodAdded".to_string(),
                CustomerEvent::PaymentMethodAdded(CustomerPaymentMethodAdded {
                    payment_method: PaymentMethod {
                        id: payment_id,
                        method_type: PaymentMethodType::CreditCard,
                        last_four: "1234".to_string(),
                        is_default: true,
                    },
                }),
                Uuid::new_v4(),
            ),
            EventEnvelope::new(
                customer_id,
                4,
                "TierUpgraded".to_string(),
                CustomerEvent::TierUpgraded(CustomerTierUpgraded {
                    old_tier: CustomerTier::Bronze,
                    new_tier: CustomerTier::Silver,
                }),
                Uuid::new_v4(),
            ),
        ];

        let aggregate = CustomerAggregate::load_from_events(events).unwrap();
        assert_eq!(aggregate.version, 4);
        assert_eq!(aggregate.tier, CustomerTier::Silver);
        assert_eq!(aggregate.addresses.len(), 1);
        assert_eq!(aggregate.payment_methods.len(), 1);
        assert_eq!(aggregate.default_address_id, Some(address_id));
    }

    #[test]
    fn test_apply_first_event_non_registered_fails() {
        let event = CustomerEvent::ProfileUpdated(CustomerProfileUpdated {
            first_name: Some("Test".to_string()),
            last_name: None,
            phone: None,
        });

        let result = CustomerAggregate::apply_first_event(&event);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CustomerError::NotInitialized));
    }

    #[test]
    fn test_all_tier_upgrades() {
        let mut aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        // Bronze -> Silver
        aggregate.apply_event(&CustomerEvent::TierUpgraded(CustomerTierUpgraded {
            old_tier: CustomerTier::Bronze,
            new_tier: CustomerTier::Silver,
        })).unwrap();
        assert_eq!(aggregate.tier, CustomerTier::Silver);

        // Silver -> Gold
        aggregate.apply_event(&CustomerEvent::TierUpgraded(CustomerTierUpgraded {
            old_tier: CustomerTier::Silver,
            new_tier: CustomerTier::Gold,
        })).unwrap();
        assert_eq!(aggregate.tier, CustomerTier::Gold);

        // Gold -> Platinum
        aggregate.apply_event(&CustomerEvent::TierUpgraded(CustomerTierUpgraded {
            old_tier: CustomerTier::Gold,
            new_tier: CustomerTier::Platinum,
        })).unwrap();
        assert_eq!(aggregate.tier, CustomerTier::Platinum);
    }

    #[test]
    fn test_change_email_no_change_returns_empty() {
        let aggregate = CustomerAggregate::apply_first_event(
            &CustomerEvent::Registered(create_test_customer())
        ).unwrap();

        let command = CustomerCommand::ChangeEmail {
            new_email: aggregate.email.clone(),
        };

        let events = aggregate.handle_command(&command).unwrap();
        assert_eq!(events.len(), 0);
    }
}

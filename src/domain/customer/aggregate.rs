use uuid::Uuid;
use std::collections::HashMap;
use anyhow::Result;

use crate::event_sourcing::core::{Aggregate, EventEnvelope};
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

impl Aggregate for CustomerAggregate {
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

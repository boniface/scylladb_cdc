use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Customer Value Objects
// ============================================================================

/// Customer email address
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Email(pub String);

impl Email {
    pub fn new(email: impl Into<String>) -> Self {
        Self(email.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Customer phone number
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhoneNumber(pub String);

impl PhoneNumber {
    pub fn new(phone: impl Into<String>) -> Self {
        Self(phone.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Customer address
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Address {
    pub street: String,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub country: String,
}

/// Customer status in the system
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CustomerStatus {
    Active,
    Suspended,
    Deactivated,
}

/// Customer tier for loyalty/pricing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CustomerTier {
    Bronze,
    Silver,
    Gold,
    Platinum,
}

/// Payment method information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaymentMethod {
    pub id: Uuid,
    pub method_type: PaymentMethodType,
    pub last_four: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PaymentMethodType {
    CreditCard,
    DebitCard,
    BankAccount,
    DigitalWallet,
}

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

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_creation() {
        let email = Email::new("test@example.com");
        assert_eq!(email.as_str(), "test@example.com");
    }

    #[test]
    fn test_email_equality() {
        let email1 = Email::new("test@example.com");
        let email2 = Email::new("test@example.com");
        let email3 = Email::new("other@example.com");

        assert_eq!(email1, email2);
        assert_ne!(email1, email3);
    }

    #[test]
    fn test_email_serialization() {
        let email = Email::new("test@example.com");
        let json = serde_json::to_string(&email).unwrap();
        let deserialized: Email = serde_json::from_str(&json).unwrap();
        assert_eq!(email, deserialized);
    }

    #[test]
    fn test_phone_number_creation() {
        let phone = PhoneNumber::new("555-1234");
        assert_eq!(phone.as_str(), "555-1234");
    }

    #[test]
    fn test_phone_number_serialization() {
        let phone = PhoneNumber::new("+1-555-1234");
        let json = serde_json::to_string(&phone).unwrap();
        let deserialized: PhoneNumber = serde_json::from_str(&json).unwrap();
        assert_eq!(phone, deserialized);
    }

    #[test]
    fn test_address_creation() {
        let address = Address {
            street: "123 Main St".to_string(),
            city: "Anytown".to_string(),
            state: "CA".to_string(),
            postal_code: "12345".to_string(),
            country: "USA".to_string(),
        };

        assert_eq!(address.street, "123 Main St");
        assert_eq!(address.city, "Anytown");
        assert_eq!(address.state, "CA");
        assert_eq!(address.postal_code, "12345");
        assert_eq!(address.country, "USA");
    }

    #[test]
    fn test_address_serialization() {
        let address = Address {
            street: "123 Main St".to_string(),
            city: "Anytown".to_string(),
            state: "CA".to_string(),
            postal_code: "12345".to_string(),
            country: "USA".to_string(),
        };

        let json = serde_json::to_string(&address).unwrap();
        let deserialized: Address = serde_json::from_str(&json).unwrap();
        assert_eq!(address, deserialized);
    }

    #[test]
    fn test_customer_status_all_values() {
        let statuses = vec![
            CustomerStatus::Active,
            CustomerStatus::Suspended,
            CustomerStatus::Deactivated,
        ];

        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: CustomerStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    #[test]
    fn test_customer_tier_all_values() {
        let tiers = vec![
            CustomerTier::Bronze,
            CustomerTier::Silver,
            CustomerTier::Gold,
            CustomerTier::Platinum,
        ];

        for tier in tiers {
            let json = serde_json::to_string(&tier).unwrap();
            let deserialized: CustomerTier = serde_json::from_str(&json).unwrap();
            assert_eq!(tier, deserialized);
        }
    }

    #[test]
    fn test_payment_method_creation() {
        let payment = PaymentMethod {
            id: Uuid::new_v4(),
            method_type: PaymentMethodType::CreditCard,
            last_four: "1234".to_string(),
            is_default: true,
        };

        assert_eq!(payment.last_four, "1234");
        assert!(payment.is_default);
        assert_eq!(payment.method_type, PaymentMethodType::CreditCard);
    }

    #[test]
    fn test_payment_method_serialization() {
        let payment = PaymentMethod {
            id: Uuid::new_v4(),
            method_type: PaymentMethodType::DebitCard,
            last_four: "5678".to_string(),
            is_default: false,
        };

        let json = serde_json::to_string(&payment).unwrap();
        let deserialized: PaymentMethod = serde_json::from_str(&json).unwrap();
        assert_eq!(payment, deserialized);
    }

    #[test]
    fn test_payment_method_types() {
        let types = vec![
            PaymentMethodType::CreditCard,
            PaymentMethodType::DebitCard,
            PaymentMethodType::BankAccount,
            PaymentMethodType::DigitalWallet,
        ];

        for method_type in types {
            let json = serde_json::to_string(&method_type).unwrap();
            let deserialized: PaymentMethodType = serde_json::from_str(&json).unwrap();
            assert_eq!(method_type, deserialized);
        }
    }

    #[test]
    fn test_address_equality() {
        let addr1 = Address {
            street: "123 Main St".to_string(),
            city: "Anytown".to_string(),
            state: "CA".to_string(),
            postal_code: "12345".to_string(),
            country: "USA".to_string(),
        };

        let addr2 = Address {
            street: "123 Main St".to_string(),
            city: "Anytown".to_string(),
            state: "CA".to_string(),
            postal_code: "12345".to_string(),
            country: "USA".to_string(),
        };

        let addr3 = Address {
            street: "456 Oak Ave".to_string(),
            city: "Anytown".to_string(),
            state: "CA".to_string(),
            postal_code: "12345".to_string(),
            country: "USA".to_string(),
        };

        assert_eq!(addr1, addr2);
        assert_ne!(addr1, addr3);
    }
}

// ============================================================================
// Customer Domain - Business Logic for Customer Aggregate
// ============================================================================
//
// This module contains ALL Customer-specific code:
// - Value objects (Email, Address, PhoneNumber, etc.)
// - Events (CustomerRegistered, CustomerSuspended, etc.)
// - Commands (RegisterCustomer, UpdateProfile, etc.)
// - Errors (CustomerError enum)
// - Aggregate (CustomerAggregate with business logic)
// - Command Handler (CustomerCommandHandler)
//
// This is completely separate from the generic event sourcing infrastructure.
//
// ============================================================================

pub mod value_objects;
pub mod events;
pub mod commands;
pub mod errors;
pub mod aggregate;
pub mod command_handler;

// Re-export for convenience
pub use value_objects::*;
pub use events::*;
pub use commands::*;
pub use errors::*;
pub use aggregate::*;
pub use command_handler::*;

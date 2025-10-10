// ============================================================================
// Order Domain - Business Logic for Order Aggregate
// ============================================================================
//
// This module contains ALL Order-specific code:
// - Value objects (OrderItem, OrderStatus)
// - Events (OrderCreated, OrderConfirmed, etc.)
// - Commands (CreateOrder, ConfirmOrder, etc.)
// - Errors (OrderError enum)
// - Aggregate (OrderAggregate with business logic)
// - Command Handler (OrderCommandHandler)
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

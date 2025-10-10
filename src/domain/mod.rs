// ============================================================================
// Domain Layer - Business Logic
// ============================================================================
//
// This module contains domain-specific aggregates and business logic.
// Each aggregate has its own subdirectory with:
// - Value objects
// - Events
// - Commands
// - Errors
// - Aggregate implementation
// - Command handler
//
// This layer is completely separate from the event sourcing infrastructure.
//
// ============================================================================

pub mod order;
pub mod customer;

// Future aggregates can be added here:
// pub mod product;
// pub mod payment;

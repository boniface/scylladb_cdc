// ============================================================================
// Event Sourcing Infrastructure
// ============================================================================
//
// Generic, reusable event sourcing infrastructure.
// Domain-specific code is in src/domain/
//
// ============================================================================

// Core abstractions (GENERIC - works with any aggregate)
mod core;
mod store;

// Re-export core infrastructure
pub use core::*;
pub use store::*;

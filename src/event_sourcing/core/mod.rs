// ============================================================================
// Event Sourcing Core - Generic Infrastructure Abstractions
// ============================================================================
//
// This module contains GENERIC, reusable event sourcing infrastructure
// that works with ANY domain aggregate.
//
// Key Principles:
// - No domain-specific code (no Order, Customer, Product, etc.)
// - Generic over aggregate types
// - Reusable across all aggregates
//
// ============================================================================

pub mod aggregate;
pub mod event;

// Re-export core types for convenience
pub use aggregate::Aggregate;
pub use event::{DomainEvent, EventEnvelope, serialize_event, deserialize_event, EventUpcaster};

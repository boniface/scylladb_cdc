// ============================================================================
// Event Sourcing Store - Generic Persistence Layer
// ============================================================================
//
// This module contains GENERIC persistence infrastructure for event sourcing.
// All components work with ANY aggregate/event type.
//
// ============================================================================

pub mod event_store;

pub use event_store::EventStore;

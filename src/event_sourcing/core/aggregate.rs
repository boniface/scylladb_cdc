use uuid::Uuid;
use anyhow::Result;
use super::event::EventEnvelope;

// ============================================================================
// Aggregate Root Pattern - Event Sourcing Core
// ============================================================================
//
// Key Principles:
// 1. State is derived from events (not stored directly)
// 2. Commands are validated before emitting events
// 3. Events represent facts that have already happened
// 4. Aggregate Roots enforce business invariants
// 5. All state changes flow through events
//
// DDD Terminology:
// - Entity: An object with unique identity
// - Aggregate Root: The entry point entity that defines the aggregate boundary
// - This trait represents AGGREGATE ROOTS (not just any entity)
//
// ============================================================================

/// Aggregate Root trait - represents the root entity of an aggregate
///
/// In DDD, an Aggregate Root is the entry point to a cluster of related entities
/// and value objects. It enforces invariants and consistency within the aggregate
/// boundary.
///
/// Type Parameters:
/// - `Event`: The domain event type for this aggregate
/// - `Command`: The command type for this aggregate
/// - `Error`: The error type for business rule violations
pub trait AggregateRoot: Sized + Send + Sync {
    type Event;
    type Command;
    type Error;

    /// Create new aggregate from first event
    fn apply_first_event(event: &Self::Event) -> Result<Self, Self::Error>;

    /// Apply subsequent events to update state
    fn apply_event(&mut self, event: &Self::Event) -> Result<(), Self::Error>;

    /// Handle command and emit events (business logic)
    fn handle_command(&self, command: &Self::Command) -> Result<Vec<Self::Event>, Self::Error>;

    /// Get aggregate ID
    fn aggregate_id(&self) -> Uuid;

    /// Get current version (sequence number)
    fn version(&self) -> i64;

    /// Load aggregate from event history (reconstruct from events)
    /// This method must be implemented by each aggregate to properly set version from events
    fn load_from_events(events: Vec<EventEnvelope<Self::Event>>) -> Result<Self>
    where
        Self::Error: std::fmt::Display;

}

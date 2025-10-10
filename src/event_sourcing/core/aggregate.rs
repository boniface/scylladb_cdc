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
// 4. Aggregates enforce business invariants
// 5. All state changes flow through events
//
// This is the GENERIC aggregate trait that works for ANY domain aggregate.
//
// ============================================================================

/// Generic Aggregate trait - all event-sourced aggregates implement this
///
/// Type Parameters:
/// - `Event`: The domain event type for this aggregate
/// - `Command`: The command type for this aggregate
/// - `Error`: The error type for business rule violations
pub trait Aggregate: Sized + Send + Sync {
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
    fn load_from_events(events: Vec<EventEnvelope<Self::Event>>) -> Result<Self>
    where
        Self::Error: std::fmt::Display,
    {
        if events.is_empty() {
            anyhow::bail!("No events to load");
        }

        // Apply first event to create aggregate
        let mut aggregate = Self::apply_first_event(&events[0].event_data)
            .map_err(|e| anyhow::anyhow!("Failed to apply first event: {}", e))?;

        // Apply remaining events
        for envelope in events.iter().skip(1) {
            aggregate.apply_event(&envelope.event_data)
                .map_err(|e| anyhow::anyhow!("Failed to apply event: {}", e))?;
        }

        Ok(aggregate)
    }
}

use actix::prelude::*;

// ============================================================================
// Supervised Actor Trait
// ============================================================================
//
// Common interface for actors that are managed by a supervisor (Coordinator).
// Provides lifecycle hooks and metadata for supervision strategies.
//
// ============================================================================

/// Supervision strategy for an actor
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SupervisionStrategy {
    /// Restart actor on failure
    Restart,
    /// Stop actor permanently on failure
    Stop,
    /// Escalate failure to parent
    Escalate,
}

/// Metadata about a supervised actor
pub struct ActorMetadata {
    pub name: String,
    pub description: String,
    pub strategy: SupervisionStrategy,
}

/// Trait for actors that can be supervised by a coordinator
pub trait SupervisedActor: Actor {
    /// Get actor metadata
    fn metadata(&self) -> ActorMetadata;

    /// Called when actor is being supervised
    fn on_supervised(&mut self, _ctx: &mut Self::Context) {
        // Default: do nothing
    }

    /// Called before actor stops
    fn on_stop(&mut self, _ctx: &mut Self::Context) {
        // Default: do nothing
    }
}

/// Message to request actor metadata
#[derive(Message)]
#[rtype(result = "ActorMetadata")]
pub struct GetMetadata;

/// Message to gracefully stop an actor
#[derive(Message)]
#[rtype(result = "()")]
pub struct GracefulStop;

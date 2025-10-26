// ============================================================================
// Supervised Actor Metadata
// ============================================================================
//
// Metadata structures for supervised actors.
// Kameo provides built-in supervision via Actor trait hooks:
// - on_start, on_stop, on_panic, on_link_died
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
#[derive(Debug, Clone)]
pub struct ActorMetadata {
    pub name: String,
    pub description: String,
    pub strategy: SupervisionStrategy,
}

// ============================================================================
// Actors Module
// ============================================================================
//
// Actor-based infrastructure for asynchronous, concurrent operations.
//
// Structure:
// - core/           - Abstract traits and types (HealthCheckable, SupervisedActor)
// - infrastructure/ - Concrete infrastructure actors (CDC, DLQ, Health, Coordinator)
//
// Note: Domain logic (Order, Customer, etc.) uses CommandHandlers, NOT actors.
//       Actors are reserved for infrastructure concerns only.
//
// ============================================================================

// Private module declarations
mod core;
mod infrastructure;

// Re-export only what's needed in the public API
pub use infrastructure::CoordinatorActor;

// Internal re-exports for use within the crate
pub(crate) use core::{HealthStatus, ComponentHealth, HealthCheckable};
pub(crate) use infrastructure::{
    CdcProcessor,
    DlqActor,
    HealthMonitorActor,
    UpdateHealth,
    GetSystemHealth,
    SystemHealth,
    AddToDlq,
};

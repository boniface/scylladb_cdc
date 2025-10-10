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

pub mod core;
pub mod infrastructure;

// Re-export commonly used types
pub use core::{HealthStatus, ComponentHealth, HealthCheckable};
pub use infrastructure::{
    CoordinatorActor,
    CdcProcessor,
    DlqActor,
    HealthMonitorActor,
    UpdateHealth,
    GetSystemHealth,
    SystemHealth,
    AddToDlq,
};

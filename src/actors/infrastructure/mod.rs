// ============================================================================
// Infrastructure Actors
// ============================================================================
//
// Reusable infrastructure actors for system concerns:
// - CDC stream processing
// - Dead letter queue
// - Health monitoring
// - Coordination and supervision
//
// ============================================================================

// Private module declarations
mod cdc_processor;
mod dlq;
mod health_monitor;
mod coordinator;

// Re-export for public API
pub use cdc_processor::CdcProcessor;
pub use dlq::{DlqActor, AddToDlq};
pub use health_monitor::{HealthMonitorActor, UpdateHealth, GetSystemHealth, SystemHealth};
pub use coordinator::CoordinatorActor;

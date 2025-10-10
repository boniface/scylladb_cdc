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

pub mod cdc_processor;
pub mod dlq;
pub mod health_monitor;
pub mod coordinator;

// Re-export for convenience
pub use cdc_processor::CdcProcessor;
pub use dlq::{DlqActor, AddToDlq, GetDlqMessages, GetDlqStats, DlqMessage, DlqStats};
pub use health_monitor::{HealthMonitorActor, UpdateHealth, GetSystemHealth, SystemHealth};
pub use coordinator::CoordinatorActor;

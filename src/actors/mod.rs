pub mod order_actor;
pub mod cdc_processor_polling;  // Phase 2: Polling-based (educational)
pub mod cdc_stream_processor;   // Phase 3: Real CDC streams
pub mod health_check;           // Phase 4: Health monitoring
pub mod coordinator;            // Phase 4: Actor supervision
pub mod dlq_actor;              // Phase 5: Dead Letter Queue

pub use order_actor::{OrderActor, CreateOrder, UpdateOrder, CancelOrder};

// Export both processors - use CdcStreamProcessor in production
pub use cdc_processor_polling::CdcProcessor as CdcPollingProcessor;
pub use cdc_stream_processor::CdcStreamProcessor;

// Phase 4: Export coordinator and health check
pub use coordinator::{CoordinatorActor, Shutdown, GetOrderActor, GetHealthCheckActor};
pub use health_check::{HealthCheckActor, HealthStatus, UpdateHealth, GetSystemHealth, SystemHealth};

// Phase 5: Export DLQ actor
pub use dlq_actor::{DlqActor, AddToDlq, GetDlqMessages, GetDlqStats, DlqMessage, DlqStats};
use kameo::Actor;
use kameo::actor::ActorRef;
use kameo::error::Infallible;
use scylla::client::session::Session;
use std::sync::Arc;
use crate::messaging::RedpandaClient;
use crate::utils::{retry_with_backoff, RetryConfig, RetryResult};
use super::{DlqActor, AddToDlq};
use uuid::Uuid;
use chrono::Utc;
use scylla_cdc::consumer::{Consumer, ConsumerFactory, CDCRow, OperationType};
use scylla_cdc::log_reader::CDCLogReaderBuilder;
use async_trait::async_trait;

// ============================================================================
// CDC Stream Processor Actor - Uses real ScyllaDB CDC streams
// ============================================================================
//
// This implementation uses the scylla-cdc library to consume CDC log tables
// in real-time, providing:
//
// 1. TRUE STREAMING: No polling, events arrive as they're written
// 2. LOW LATENCY: Near real-time event delivery
// 3. GENERATION HANDLING: Automatically handles CDC generation changes
// 4. ORDERED DELIVERY: Respects CDC stream ordering guarantees
// 5. FAULT TOLERANCE: Built-in checkpointing and resumption
//
// How it works:
// - ScyllaDB CDC creates hidden log tables for each CDC-enabled table
// - The scylla-cdc library reads from these log tables continuously
// - We implement the Consumer trait to process each CDC row
// - Each row represents a change (insert/update/delete) to outbox_messages
// - We extract the event data and publish to Redpanda
//
// ============================================================================

const KEYSPACE: &str = "orders_ks";
const TABLE: &str = "outbox_messages";

/// Our custom consumer that processes CDC rows from outbox_messages table
pub(crate) struct OutboxCDCConsumer {
    redpanda: Arc<RedpandaClient>,
    dlq_actor: Option<ActorRef<DlqActor>>,
    retry_config: RetryConfig,
}

impl OutboxCDCConsumer {
    pub fn new(redpanda: Arc<RedpandaClient>, dlq_actor: Option<ActorRef<DlqActor>>) -> Self {
        Self {
            redpanda,
            dlq_actor,
            retry_config: RetryConfig::aggressive(), // More retries for CDC events
        }
    }

    /// Extract event data from a CDC row
    /// CDC rows contain the actual data that was inserted into outbox_messages
    fn extract_event_from_cdc_row(&self, data: &CDCRow<'_>) -> anyhow::Result<Option<OutboxEvent>> {
        // Only process inserts - we don't care about updates/deletes on outbox table
        match data.operation {
            OperationType::RowInsert | OperationType::PostImage => {
                // Extract the columns from the CDC row
                let id = data.get_value("id")
                    .as_ref()
                    .and_then(|v| v.as_uuid())
                    .ok_or_else(|| anyhow::anyhow!("Missing or invalid id"))?;

                let aggregate_id = data.get_value("aggregate_id")
                    .as_ref()
                    .and_then(|v| v.as_uuid())
                    .ok_or_else(|| anyhow::anyhow!("Missing or invalid aggregate_id"))?;

                let event_type = data.get_value("event_type")
                    .as_ref()
                    .and_then(|v| v.as_text())
                    .map(|s| s.to_string())
                    .ok_or_else(|| anyhow::anyhow!("Missing or invalid event_type"))?;

                let payload = data.get_value("payload")
                    .as_ref()
                    .and_then(|v| v.as_text())
                    .map(|s| s.to_string())
                    .ok_or_else(|| anyhow::anyhow!("Missing or invalid payload"))?;

                tracing::debug!(
                    event_id = %id,
                    event_type = %event_type,
                    aggregate_id = %aggregate_id,
                    cdc_operation = %data.operation,
                    "Extracted event from CDC row"
                );

                Ok(Some(OutboxEvent {
                    id,
                    aggregate_id,
                    event_type,
                    payload,
                }))
            }
            _ => {
                // Ignore updates, deletes, etc.
                tracing::debug!(
                    cdc_operation = %data.operation,
                    "Skipping non-insert CDC operation"
                );
                Ok(None)
            }
        }
    }
}

#[derive(Debug)]
struct OutboxEvent {
    id: Uuid,
    aggregate_id: Uuid,
    event_type: String,
    payload: String,
}

#[async_trait]
impl Consumer for OutboxCDCConsumer {
    async fn consume_cdc(&mut self, data: CDCRow<'_>) -> anyhow::Result<()> {
        tracing::debug!(
            stream_id = ?data.stream_id,
            operation = %data.operation,
            "Received CDC row"
        );

        // Extract event from CDC row
        match self.extract_event_from_cdc_row(&data)? {
            Some(event) => {
                tracing::info!(
                    event_id = %event.id,
                    event_type = %event.event_type,
                    aggregate_id = %event.aggregate_id,
                    "📤 Publishing event from CDC stream to Redpanda"
                );

                // Publish with retry
                let redpanda = self.redpanda.clone();
                let event_type = event.event_type.clone();
                let event_id = event.id;
                let aggregate_id = event.aggregate_id;
                let payload = event.payload.clone();
                let first_attempt_time = Utc::now();

                let result = retry_with_backoff(
                    self.retry_config.clone(),
                    |attempt| {
                        let redpanda = redpanda.clone();
                        let event_type = event_type.clone();
                        let event_id_str = event_id.to_string();
                        let payload = payload.clone();

                        async move {
                            tracing::debug!(
                                attempt = attempt,
                                event_id = %event_id,
                                "Attempting to publish event"
                            );

                            redpanda.publish(&event_type, &event_id_str, &payload).await
                        }
                    }
                ).await;

                match result {
                    RetryResult::Success(_) => {
                        tracing::info!(
                            event_id = %event_id,
                            event_type = %event_type,
                            "✅ Successfully published event via CDC stream"
                        );
                        Ok(())
                    }
                    RetryResult::Failed(e) | RetryResult::PermanentFailure(e) => {
                        tracing::error!(
                            error = %e,
                            event_id = %event_id,
                            event_type = %event_type,
                            "❌ Failed to publish event after retries, sending to DLQ"
                        );

                        // Send to Dead Letter Queue
                        if let Some(ref dlq) = self.dlq_actor {
                            // Fire and forget - use tell
                            let _ = dlq.tell(AddToDlq {
                                id: event_id,
                                aggregate_id,
                                event_type: event_type.clone(),
                                payload,
                                error_message: e.to_string(),
                                failure_count: self.retry_config.max_attempts as i32,
                                first_failed_at: first_attempt_time,
                            }).send().await;
                        }

                        // Don't propagate error - message is in DLQ for manual handling
                        Ok(())
                    }
                }
            }
            None => {
                // Non-insert operation, nothing to publish
                Ok(())
            }
        }
    }
}

/// Factory for creating consumer instances
/// The scylla-cdc library will create one consumer per VNode group
pub(crate) struct OutboxConsumerFactory {
    redpanda: Arc<RedpandaClient>,
    dlq_actor: Option<ActorRef<DlqActor>>,
}

impl OutboxConsumerFactory {
    pub fn new(redpanda: Arc<RedpandaClient>, dlq_actor: Option<ActorRef<DlqActor>>) -> Self {
        Self { redpanda, dlq_actor }
    }
}

#[async_trait]
impl ConsumerFactory for OutboxConsumerFactory {
    async fn new_consumer(&self) -> Box<dyn Consumer> {
        tracing::debug!("Creating new OutboxCDCConsumer instance");
        Box::new(OutboxCDCConsumer::new(self.redpanda.clone(), self.dlq_actor.clone()))
    }
}

// ============================================================================
// CDC Processor Actor
// ============================================================================

pub struct CdcProcessor {
    session: Arc<Session>,
    redpanda: Arc<RedpandaClient>,
    dlq_actor: Option<ActorRef<DlqActor>>,
}

impl CdcProcessor {
    pub fn new(session: Arc<Session>, redpanda: Arc<RedpandaClient>, dlq_actor: Option<ActorRef<DlqActor>>) -> Self {
        Self { session, redpanda, dlq_actor }
    }

    /// Start the CDC log reader
    /// This will continuously stream changes from the CDC log
    pub async fn start_cdc_streaming(&self) -> anyhow::Result<()> {
        tracing::info!("🔄 Starting CDC streaming for outbox_messages table");
        tracing::info!("📊 This uses real ScyllaDB CDC streams with retry and DLQ!");

        let factory = Arc::new(OutboxConsumerFactory::new(self.redpanda.clone(), self.dlq_actor.clone()));

        // Build the CDC log reader
        // It will start reading from "now" and continue forever
        let (_reader, handle) = CDCLogReaderBuilder::new()
            .session(self.session.clone())
            .keyspace(KEYSPACE)
            .table_name(TABLE)
            .consumer_factory(factory)
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create CDC log reader: {}", e))?;

        tracing::info!("✅ CDC log reader started successfully");
        tracing::info!("🎯 Listening for changes to {}.{}", KEYSPACE, TABLE);

        // Spawn the handle to run in the background
        tokio::spawn(async move {
            match handle.await {
                Ok(_) => {
                    tracing::info!("CDC reader completed successfully");
                }
                Err(e) => {
                    tracing::error!(error = %e, "CDC reader failed");
                }
            }
        });

        Ok(())
    }
}

impl Actor for CdcProcessor {
    type Args = Self;
    type Error = Infallible;

    async fn on_start(
        state: Self::Args,
        _actor_ref: ActorRef<Self>
    ) -> Result<Self, Self::Error> {
        tracing::info!("CdcProcessor actor started");

        let session = state.session.clone();
        let redpanda = state.redpanda.clone();
        let dlq_actor = state.dlq_actor.clone();

        tokio::spawn(async move {
            let processor = CdcProcessor::new(session, redpanda, dlq_actor);
            if let Err(e) = processor.start_cdc_streaming().await {
                tracing::error!("Failed to start CDC streaming: {}", e);
            }
        });

        Ok(state)
    }
}

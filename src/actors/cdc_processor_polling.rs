use actix::prelude::*;
use scylla::client::session::Session;
use std::sync::Arc;
use std::collections::HashSet;
use crate::messaging::RedpandaClient;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use tokio::time::{sleep, Duration};

// ============================================================================
// CDC Processor Actor - Polls outbox table and publishes to Redpanda
// ============================================================================
//
// EDUCATIONAL NOTE: This is a simplified polling-based approach for learning.
// In Phase 3, we'll replace this with real ScyllaDB CDC streams.
//
// Current Implementation:
// 1. Polls outbox_messages table periodically
// 2. Tracks last processed event ID to avoid reprocessing
// 3. Publishes events to Redpanda/Kafka
// 4. Stores offset in cdc_offsets table for resumability
//
// Limitations of this approach:
// - Uses polling instead of real-time streaming
// - Higher latency than true CDC
// - More load on database
//
// ============================================================================

const CONSUMER_ID: &str = "outbox-processor-v1";
const POLL_INTERVAL_SECS: u64 = 2;

pub struct CdcProcessor {
    session: Arc<Session>,
    redpanda: Arc<RedpandaClient>,
}

#[derive(Debug)]
struct OutboxMessage {
    id: Uuid,
    aggregate_id: Uuid,
    event_type: String,
    payload: String,
    created_at: DateTime<Utc>,
}

impl CdcProcessor {
    pub fn new(session: Arc<Session>, redpanda: Arc<RedpandaClient>) -> Self {
        Self { session, redpanda }
    }

    /// Load the last processed position from the offset table
    async fn load_offset(&self) -> anyhow::Result<Option<(DateTime<Utc>, Uuid)>> {
        let result = self.session
            .query_unpaged(
                "SELECT last_processed_time, last_event_id FROM cdc_offsets WHERE consumer_id = ? AND table_name = ?",
                (CONSUMER_ID, "outbox_messages"),
            )
            .await?;

        let rows_result = result.into_rows_result()?;
        let rows = rows_result.rows()?;
        if let Some(row) = rows.into_iter().next() {
            let (time, id): (DateTime<Utc>, Uuid) = row?;
            tracing::info!(
                last_processed_time = %time,
                last_event_id = %id,
                "Loaded CDC offset from storage"
            );
            return Ok(Some((time, id)));
        }

        tracing::info!("No previous offset found, starting from current time");
        Ok(None)
    }

    /// Save the current processing position
    async fn save_offset(&self, last_time: DateTime<Utc>, last_id: Uuid) -> anyhow::Result<()> {
        self.session
            .query_unpaged(
                "INSERT INTO cdc_offsets (consumer_id, table_name, last_processed_time, last_event_id, updated_at) VALUES (?, ?, ?, ?, ?)",
                (CONSUMER_ID, "outbox_messages", last_time, last_id, Utc::now()),
            )
            .await?;

        tracing::debug!(
            last_processed_time = %last_time,
            last_event_id = %last_id,
            "Saved CDC offset"
        );

        Ok(())
    }

    /// Fetch new outbox messages since the last processed time
    /// Uses a time-based query to avoid ALLOW FILTERING
    async fn fetch_new_messages(&self, since: DateTime<Utc>) -> anyhow::Result<Vec<OutboxMessage>> {
        // Note: This query is still inefficient without a proper index on created_at
        // In production, you'd either:
        // 1. Use a materialized view with created_at in the partition key
        // 2. Use the real CDC streams (Phase 3)
        // 3. Add a time-bucketing strategy (e.g., partition by hour)

        let result = self.session
            .query_unpaged(
                "SELECT id, aggregate_id, event_type, payload, created_at FROM outbox_messages WHERE created_at > ? ALLOW FILTERING",
                (since,),
            )
            .await?;

        let mut messages = Vec::new();

        let rows_result = result.into_rows_result()?;
        let rows = rows_result.rows()?;
        for row in rows {
            let (id, aggregate_id, event_type, payload, created_at): (Uuid, Uuid, String, String, DateTime<Utc>) =
                row?;

            messages.push(OutboxMessage {
                id,
                aggregate_id,
                event_type,
                payload,
                created_at,
            });
        }

        // Sort by timestamp to process in order
        messages.sort_by_key(|m| m.created_at);

        Ok(messages)
    }

    /// Main polling loop
    pub async fn start_cdc_monitoring(&self) -> anyhow::Result<()> {
        tracing::info!("🔄 Starting CDC monitoring for outbox_messages table");

        let session = self.session.clone();
        let redpanda = self.redpanda.clone();

        tokio::spawn(async move {
            let processor = CdcProcessor::new(session, redpanda);

            // Load last offset or start from now
            let (mut last_processed_time, mut processed_ids) = match processor.load_offset().await {
                Ok(Some((time, id))) => {
                    let mut set = HashSet::new();
                    set.insert(id);
                    (time, set)
                },
                _ => (Utc::now(), HashSet::new()),
            };

            loop {
                match processor.fetch_new_messages(last_processed_time).await {
                    Ok(messages) => {
                        if !messages.is_empty() {
                            tracing::info!(
                                message_count = messages.len(),
                                "📬 Fetched new outbox messages"
                            );
                        }

                        for msg in messages {
                            // Idempotency check: skip if already processed
                            if processed_ids.contains(&msg.id) {
                                tracing::debug!(
                                    event_id = %msg.id,
                                    "⏭️  Skipping already processed event"
                                );
                                continue;
                            }

                            tracing::info!(
                                event_id = %msg.id,
                                event_type = %msg.event_type,
                                aggregate_id = %msg.aggregate_id,
                                "📤 Publishing event to Redpanda"
                            );

                            // Publish to Redpanda
                            match processor.redpanda.publish(&msg.event_type, &msg.id.to_string(), &msg.payload).await {
                                Ok(_) => {
                                    tracing::info!(
                                        event_id = %msg.id,
                                        event_type = %msg.event_type,
                                        "✅ Successfully published event"
                                    );

                                    // Update offset tracking
                                    last_processed_time = msg.created_at;
                                    processed_ids.insert(msg.id);

                                    // Limit memory: keep only recent IDs
                                    if processed_ids.len() > 1000 {
                                        processed_ids.clear();
                                    }

                                    // Save offset periodically
                                    if let Err(e) = processor.save_offset(msg.created_at, msg.id).await {
                                        tracing::error!(error = %e, "Failed to save offset");
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        error = %e,
                                        event_id = %msg.id,
                                        event_type = %msg.event_type,
                                        "❌ Failed to publish event to Redpanda"
                                    );
                                    // In production: implement retry with exponential backoff
                                    // For now, we'll continue and try again on next poll
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to fetch outbox messages");
                    }
                }

                // Wait before next poll
                sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
            }
        });

        Ok(())
    }
}

impl Actor for CdcProcessor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("CdcProcessor actor started");
        let session = self.session.clone();
        let redpanda = self.redpanda.clone();

        ctx.spawn(async move {
            let processor = CdcProcessor::new(session, redpanda);
            if let Err(e) = processor.start_cdc_monitoring().await {
                tracing::error!("Failed to start CDC monitoring: {}", e);
            }
        }.into_actor(self));
    }
}
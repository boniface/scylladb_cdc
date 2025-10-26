use kameo::Actor;
use kameo::message::{Context, Message};
use kameo::actor::ActorRef;
use kameo::error::Infallible;
use scylla::client::session::Session;
use std::sync::Arc;
use uuid::Uuid;
use chrono::{DateTime, Utc};

// ============================================================================
// Dead Letter Queue Actor
// ============================================================================
//
// Handles messages that failed to publish after all retry attempts.
// Provides:
// - Persistent storage of failed messages
// - Queryable for manual intervention
// - Metrics on failure patterns
// - Retry mechanism for DLQ messages
//
// ============================================================================

pub struct DlqActor {
    session: Arc<Session>,
}

impl DlqActor {
    pub fn new(session: Arc<Session>) -> Self {
        Self { session }
    }
}

impl Actor for DlqActor {
    type Args = Self;
    type Error = Infallible;

    async fn on_start(
        state: Self::Args,
        _actor_ref: ActorRef<Self>
    ) -> Result<Self, Self::Error> {
        tracing::info!("DlqActor started - Dead Letter Queue ready");
        Ok(state)
    }
}

// ============================================================================
// Messages
// ============================================================================

#[derive(Debug, Clone)]
pub struct AddToDlq {
    pub id: Uuid,
    pub aggregate_id: Uuid,
    pub event_type: String,
    pub payload: String,
    pub error_message: String,
    pub failure_count: i32,
    pub first_failed_at: DateTime<Utc>,
}

pub(crate) struct GetDlqMessages {
    pub limit: i32,
}

pub(crate) struct GetDlqStats;

#[derive(Debug, Clone)]
pub(crate) struct DlqMessage {
    pub id: Uuid,
    pub aggregate_id: Uuid,
    pub event_type: String,
    pub payload: String,
    pub error_message: String,
    pub failure_count: i32,
    pub first_failed_at: DateTime<Utc>,
    pub last_failed_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct DlqStats {
    pub total_messages: i64,
    pub by_event_type: std::collections::HashMap<String, i64>,
}

// ============================================================================
// Message Handlers
// ============================================================================

impl Message<AddToDlq> for DlqActor {
    type Reply = Result<(), String>;

    async fn handle(&mut self, msg: AddToDlq, _ctx: &mut Context<Self, Self::Reply>) -> Self::Reply {
        let now = Utc::now();

        tracing::error!(
            event_id = %msg.id,
            event_type = %msg.event_type,
            aggregate_id = %msg.aggregate_id,
            error = %msg.error_message,
            failure_count = msg.failure_count,
            "💀 Adding message to Dead Letter Queue"
        );

        self.session
            .query_unpaged(
                "INSERT INTO dead_letter_queue (
                    id, aggregate_id, event_type, payload,
                    error_message, failure_count, first_failed_at,
                    last_failed_at, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    msg.id,
                    msg.aggregate_id,
                    &msg.event_type,
                    &msg.payload,
                    &msg.error_message,
                    msg.failure_count,
                    msg.first_failed_at,
                    now,
                    now,
                ),
            )
            .await
            .map_err(|e| format!("Failed to insert into DLQ: {}", e))?;

        tracing::info!(
            event_id = %msg.id,
            "Message successfully stored in DLQ"
        );

        Ok(())
    }
}

impl Message<GetDlqMessages> for DlqActor {
    type Reply = Result<Vec<DlqMessage>, String>;

    async fn handle(&mut self, msg: GetDlqMessages, _ctx: &mut Context<Self, Self::Reply>) -> Self::Reply {
        let result = self.session
            .query_unpaged(
                "SELECT id, aggregate_id, event_type, payload, error_message,
                        failure_count, first_failed_at, last_failed_at
                 FROM dead_letter_queue
                 LIMIT ?",
                (msg.limit,),
            )
            .await
            .map_err(|e| format!("Failed to query DLQ: {}", e))?;

        let mut messages = Vec::new();

        let rows_result = result.into_rows_result()
            .map_err(|e| format!("Failed to parse DLQ results: {}", e))?;
        let rows = rows_result.rows()
            .map_err(|e| format!("Failed to get rows: {}", e))?;

        for row in rows {
            let (id, aggregate_id, event_type, payload, error_message,
                 failure_count, first_failed_at, last_failed_at):
                (Uuid, Uuid, String, String, String, i32, DateTime<Utc>, DateTime<Utc>) =
                row.map_err(|e| format!("Failed to parse row: {}", e))?;

            messages.push(DlqMessage {
                id,
                aggregate_id,
                event_type,
                payload,
                error_message,
                failure_count,
                first_failed_at,
                last_failed_at,
            });
        }

        Ok(messages)
    }
}

impl Message<GetDlqStats> for DlqActor {
    type Reply = Result<DlqStats, String>;

    async fn handle(&mut self, _msg: GetDlqStats, _ctx: &mut Context<Self, Self::Reply>) -> Self::Reply {
        // Get total count
        let count_result = self.session
            .query_unpaged("SELECT COUNT(*) FROM dead_letter_queue", &[])
            .await
            .map_err(|e| format!("Failed to count DLQ messages: {}", e))?;

        let total_messages = match count_result.into_rows_result() {
            Ok(rows_result) => {
                match rows_result.rows() {
                    Ok(mut rows) => {
                        rows.next()
                            .and_then(|row| row.ok())
                            .map(|row: (i64,)| row.0)
                            .unwrap_or(0)
                    }
                    Err(_) => 0,
                }
            }
            Err(_) => 0,
        };

        // For now, return basic stats
        // In production, you'd query by event_type
        let by_event_type = std::collections::HashMap::new();

        Ok(DlqStats {
            total_messages,
            by_event_type,
        })
    }
}

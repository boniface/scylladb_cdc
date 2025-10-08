use actix::prelude::*;
use scylla::statement::batch::Batch;
use scylla::client::session::Session;
use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;
use crate::models::{OrderItem, OrderCreatedEvent, OrderUpdatedEvent, OrderCancelledEvent, DomainEvent};

// ============================================================================
// Actor Messages
// ============================================================================

#[derive(Message)]
#[rtype(result = "anyhow::Result<Uuid>")]
pub struct CreateOrder {
    pub customer_id: Uuid,
    pub items: Vec<OrderItem>,
}

#[derive(Message)]
#[rtype(result = "anyhow::Result<()>")]
pub struct UpdateOrder {
    pub order_id: Uuid,
    pub items: Vec<OrderItem>,
}

#[derive(Message)]
#[rtype(result = "anyhow::Result<()>")]
pub struct CancelOrder {
    pub order_id: Uuid,
    pub reason: Option<String>,
}

// ============================================================================
// Order Actor - Manages order lifecycle with transactional outbox pattern
// ============================================================================

pub struct OrderActor {
    session: Arc<Session>,
}

impl OrderActor {
    pub fn new(session: Arc<Session>) -> Self {
        Self { session }
    }

    /// Helper function to atomically write domain state + outbox event
    /// This ensures the outbox pattern's transactional guarantee:
    /// Either BOTH the order and event are persisted, or NEITHER are.
    async fn persist_with_outbox<E: DomainEvent + serde::Serialize>(
        &self,
        order_query: &str,
        order_values: impl scylla::serialize::row::SerializeRow,
        event: &E,
    ) -> anyhow::Result<()> {
        let event_id = Uuid::new_v4();
        let payload = serde_json::to_string(event)?;
        let timestamp = Utc::now();

        // Create a batch to ensure atomicity
        // ScyllaDB will either commit both writes or neither
        let mut batch = Batch::default();
        batch.append_statement(order_query);
        batch.append_statement(
            "INSERT INTO outbox_messages (id, aggregate_id, event_type, payload, created_at) VALUES (?, ?, ?, ?, ?)"
        );

        tracing::debug!(
            order_id = %event.aggregate_id(),
            event_type = %event.event_type(),
            event_id = %event_id,
            "Persisting order with outbox event"
        );

        self.session
            .batch(
                &batch,
                (
                    order_values,
                    (
                        event_id,
                        event.aggregate_id(),
                        event.event_type(),
                        payload,
                        timestamp,
                    ),
                ),
            )
            .await?;

        tracing::info!(
            order_id = %event.aggregate_id(),
            event_type = %event.event_type(),
            event_id = %event_id,
            "✅ Transactionally persisted order and outbox event"
        );

        Ok(())
    }
}

impl Actor for OrderActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("OrderActor started");
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

impl Handler<CreateOrder> for OrderActor {
    type Result = ResponseFuture<anyhow::Result<Uuid>>;

    fn handle(&mut self, msg: CreateOrder, _: &mut Self::Context) -> Self::Result {
        let session = self.session.clone();
        let customer_id = msg.customer_id;
        let items = msg.items;

        Box::pin(async move {
            let order_id = Uuid::new_v4();
            let timestamp = Utc::now();

            tracing::info!(
                order_id = %order_id,
                customer_id = %customer_id,
                item_count = items.len(),
                "Creating new order"
            );

            let event = OrderCreatedEvent {
                order_id,
                customer_id,
                items: items.clone(),
                timestamp,
            };

            let actor = OrderActor { session };

            actor.persist_with_outbox(
                "INSERT INTO orders (id, customer_id, items, created_at) VALUES (?, ?, ?, ?)",
                (order_id, customer_id, serde_json::to_string(&items)?, timestamp),
                &event,
            ).await?;

            Ok(order_id)
        })
    }
}

impl Handler<UpdateOrder> for OrderActor {
    type Result = ResponseFuture<anyhow::Result<()>>;

    fn handle(&mut self, msg: UpdateOrder, _: &mut Self::Context) -> Self::Result {
        let session = self.session.clone();
        let order_id = msg.order_id;
        let items = msg.items;

        Box::pin(async move {
            tracing::info!(
                order_id = %order_id,
                item_count = items.len(),
                "Updating order"
            );

            let event = OrderUpdatedEvent {
                order_id,
                items: items.clone(),
                timestamp: Utc::now(),
            };

            let actor = OrderActor { session };

            actor.persist_with_outbox(
                "UPDATE orders SET items = ? WHERE id = ?",
                (serde_json::to_string(&items)?, order_id),
                &event,
            ).await?;

            Ok(())
        })
    }
}

impl Handler<CancelOrder> for OrderActor {
    type Result = ResponseFuture<anyhow::Result<()>>;

    fn handle(&mut self, msg: CancelOrder, _: &mut Self::Context) -> Self::Result {
        let session = self.session.clone();
        let order_id = msg.order_id;
        let reason = msg.reason;

        Box::pin(async move {
            tracing::info!(
                order_id = %order_id,
                reason = ?reason,
                "Cancelling order"
            );

            let event = OrderCancelledEvent {
                order_id,
                reason: reason.clone(),
                timestamp: Utc::now(),
            };

            let actor = OrderActor { session };

            actor.persist_with_outbox(
                "DELETE FROM orders WHERE id = ?",
                (order_id,),
                &event,
            ).await?;

            Ok(())
        })
    }
}
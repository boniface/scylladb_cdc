use actix::prelude::*;
use scylla::client::session::Session;
use scylla::client::session_builder::SessionBuilder;
use std::sync::Arc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod models;
mod actors;
mod messaging;
mod utils;
mod metrics;
mod db;

use actors::{CoordinatorActor, GetOrderActor};
use messaging::RedpandaClient;

#[actix::main]
async fn main() -> anyhow::Result<()> {
    // Initialize structured logging with environment-based filtering
    // Default to INFO level, can be overridden with RUST_LOG env var
    // Example: RUST_LOG=debug cargo run
    tracing_subscriber::registry()
        .with(fmt::layer().with_target(true).with_thread_ids(true))
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,scylladb_cdc=debug"))
        )
        .init();

    tracing::info!("🚀 Starting ScyllaDB CDC Outbox Pattern Demo");
    tracing::info!("📡 Phase 3: Real CDC Streams");
    tracing::info!("🎯 Phase 4: Actor Supervision & Circuit Breaker");
    tracing::info!("📊 Phase 5: DLQ, Retry, & Metrics");

    // === 1. Create ScyllaDB Session (using latest driver patterns) ===
    tracing::info!("Connecting to ScyllaDB...");
    let session: Session = SessionBuilder::new()
        .known_node("127.0.0.1:9042") // Adjust if your Scylla runs elsewhere
        .build()
        .await?;

    // Ensure keyspace exists (optional, or do this via cqlsh)
    session
        .query_unpaged(
            "CREATE KEYSPACE IF NOT EXISTS orders_ks WITH REPLICATION = \
             {'class': 'SimpleStrategy', 'replication_factor': 1}",
            &[],
        )
        .await?;

    session.use_keyspace("orders_ks", false).await?;

    let session = Arc::new(session); // Wrap for sharing

    // === 2. Initialize Prometheus metrics ===
    tracing::info!("Initializing metrics");
    let metrics = Arc::new(metrics::Metrics::new()?);
    tracing::info!("📊 Metrics registry created with {} metrics", metrics.registry().gather().len());

    // Start metrics HTTP server in background thread
    let metrics_registry = Arc::new(metrics.registry().clone());
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            if let Err(e) = metrics::start_metrics_server(metrics_registry, 9090).await {
                tracing::error!("Metrics server error: {}", e);
            }
        });
    });

    // === 3. Create Redpanda client (with circuit breaker) ===
    let redpanda = Arc::new(RedpandaClient::new("127.0.0.1:9092"));

    // === 4. Start Coordinator Actor (Phase 4: Supervision) ===
    // The coordinator manages all child actors with supervision
    tracing::info!("Starting coordinator actor with supervision");
    let coordinator = CoordinatorActor::new(session.clone(), redpanda.clone()).start();

    // === 5. Get Order Actor from Coordinator ===
    let order_actor = coordinator
        .send(GetOrderActor)
        .await?
        .expect("Order actor should be started by coordinator");

    // === 6. Demonstrate full order lifecycle ===
    tracing::info!("📝 Demonstrating order lifecycle with outbox pattern");

    // Create an order
    let customer_id = uuid::Uuid::new_v4();
    let order_id = order_actor
        .send(actors::CreateOrder {
            customer_id,
            items: vec![
                models::OrderItem {
                    product_id: uuid::Uuid::new_v4(),
                    quantity: 2,
                },
                models::OrderItem {
                    product_id: uuid::Uuid::new_v4(),
                    quantity: 1,
                },
            ],
        })
        .await??;

    tracing::info!("✅ Order created: {}", order_id);

    // Wait a bit for CDC to process
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Update the order
    order_actor
        .send(actors::UpdateOrder {
            order_id,
            items: vec![
                models::OrderItem {
                    product_id: uuid::Uuid::new_v4(),
                    quantity: 5,
                },
            ],
        })
        .await??;

    tracing::info!("✅ Order updated: {}", order_id);

    // Wait a bit
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Cancel the order
    order_actor
        .send(actors::CancelOrder {
            order_id,
            reason: Some("Customer requested cancellation".to_string()),
        })
        .await??;

    tracing::info!("✅ Order cancelled: {}", order_id);

    // Keep the app alive to let CDC process remaining events
    tracing::info!("⏳ Waiting for CDC processor to publish events...");
    tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

    tracing::info!("🎉 Demo complete!");

    Ok(())
}
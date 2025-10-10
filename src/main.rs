use actix::prelude::*;
use scylla::client::session::Session;
use scylla::client::session_builder::SessionBuilder;
use std::sync::Arc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod actors;
mod messaging;
mod utils;
mod metrics;
mod db;
mod event_sourcing;
mod domain;

use actors::CoordinatorActor;
use messaging::RedpandaClient;

// Use new domain-layered structure
use event_sourcing::store::EventStore;
use domain::order::{OrderCommandHandler, OrderCommand, OrderItem, OrderEvent};

#[actix::main]
async fn main() -> anyhow::Result<()> {
    // Initialize structured logging
    tracing_subscriber::registry()
        .with(fmt::layer().with_target(true).with_thread_ids(true))
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,scylladb_cdc=debug"))
        )
        .init();

    tracing::info!("🚀 Starting ScyllaDB Event Sourcing with CDC");
    tracing::info!("📊 Event Sourcing + CQRS + Direct CDC Projections");

    // === 1. Create ScyllaDB Session ===
    tracing::info!("Connecting to ScyllaDB...");
    let session: Session = SessionBuilder::new()
        .known_node("127.0.0.1:9042")
        .build()
        .await?;

    // Use existing keyspace (created by schema.cql via `make reset` or `make schema`)
    session.use_keyspace("orders_ks", false).await?;

    let session = Arc::new(session);

    // === 2. Initialize Prometheus metrics ===
    tracing::info!("Initializing metrics");
    let metrics = Arc::new(metrics::Metrics::new()?);
    tracing::info!("📊 Metrics registry created");

    // Start metrics HTTP server in background
    let metrics_registry = Arc::new(metrics.registry().clone());
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            if let Err(e) = metrics::start_metrics_server(metrics_registry, 9090).await {
                tracing::error!("Metrics server error: {}", e);
            }
        });
    });

    // === 3. Create Redpanda client ===
    let redpanda = Arc::new(RedpandaClient::new("127.0.0.1:9092"));

    // === 4. Start Coordinator Actor (manages CDC processor, DLQ, health check) ===
    tracing::info!("Starting coordinator actor with supervision");
    let _coordinator = CoordinatorActor::new(session.clone(), redpanda.clone()).start();

    // === 5. Initialize Event Sourcing Components ===
    tracing::info!("🎯 Initializing Event Sourcing");

    // Create Order event store (generic EventStore<OrderEvent>)
    let event_store = Arc::new(EventStore::<OrderEvent>::new(
        session.clone(),
        "Order",         // aggregate type name
        "order-events"   // topic name
    ));

    // Create Order command handler
    let command_handler = Arc::new(OrderCommandHandler::new(event_store.clone()));

    // === 6. Event Sourcing Demo ===
    tracing::info!("");
    tracing::info!("════════════════════════════════════════════════════════════");
    tracing::info!("📝 Event Sourcing Demo - Full Order Lifecycle");
    tracing::info!("════════════════════════════════════════════════════════════");
    tracing::info!("");

    let order_id = uuid::Uuid::new_v4();
    let customer_id = uuid::Uuid::new_v4();
    let correlation_id = uuid::Uuid::new_v4();

    // Create Order
    tracing::info!("1️⃣  Creating order via Event Sourcing CommandHandler...");
    let version = command_handler.handle(
        order_id,
        OrderCommand::CreateOrder {
            order_id,
            customer_id,
            items: vec![
                OrderItem {
                    product_id: uuid::Uuid::new_v4(),
                    quantity: 2,
                },
                OrderItem {
                    product_id: uuid::Uuid::new_v4(),
                    quantity: 1,
                },
            ],
        },
        correlation_id,
    ).await?;

    tracing::info!("   ✅ Order created: {} (version: {})", order_id, version);
    tracing::info!("   📦 Events written to event_store table");
    tracing::info!("   📤 Events written to outbox_messages table (atomic)");
    tracing::info!("   🌊 CDC will stream to projections and Redpanda");
    tracing::info!("");

    // Wait for CDC to process
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Confirm Order
    tracing::info!("2️⃣  Confirming order...");
    let version = command_handler.handle(
        order_id,
        OrderCommand::ConfirmOrder,
        correlation_id,
    ).await?;

    tracing::info!("   ✅ Order confirmed (version: {})", version);
    tracing::info!("");

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Ship Order
    tracing::info!("3️⃣  Shipping order...");
    let version = command_handler.handle(
        order_id,
        OrderCommand::ShipOrder {
            tracking_number: "TRACK-123-XYZ".to_string(),
            carrier: "DHL Express".to_string(),
        },
        correlation_id,
    ).await?;

    tracing::info!("   ✅ Order shipped (version: {})", version);
    tracing::info!("   📦 Tracking: TRACK-123-XYZ (DHL Express)");
    tracing::info!("");

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Deliver Order
    tracing::info!("4️⃣  Delivering order...");
    let version = command_handler.handle(
        order_id,
        OrderCommand::DeliverOrder {
            signature: Some("John Doe".to_string()),
        },
        correlation_id,
    ).await?;

    tracing::info!("   ✅ Order delivered (version: {})", version);
    tracing::info!("   ✍️  Signed by: John Doe");
    tracing::info!("");

    // Check aggregate exists
    let exists = event_store.aggregate_exists(order_id).await?;
    tracing::info!("5️⃣  Aggregate verification: {}", if exists { "✅ EXISTS" } else { "❌ NOT FOUND" });
    tracing::info!("");

    // Keep app alive for CDC processing
    tracing::info!("⏳ Waiting for CDC processor to publish events...");
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    tracing::info!("");
    tracing::info!("════════════════════════════════════════════════════════════");
    tracing::info!("🎉 Event Sourcing Demo Complete!");
    tracing::info!("════════════════════════════════════════════════════════════");
    tracing::info!("");
    tracing::info!("Event Sourcing Features Demonstrated:");
    tracing::info!("  ✅ Command Handler with business logic");
    tracing::info!("  ✅ Domain events (Created, Confirmed, Shipped, Delivered)");
    tracing::info!("  ✅ Event Store as source of truth");
    tracing::info!("  ✅ Optimistic concurrency control (versioning)");
    tracing::info!("  ✅ Atomic write to event_store + outbox_messages");
    tracing::info!("  ✅ CDC streaming to Redpanda");
    tracing::info!("  ✅ Event metadata (causation, correlation)");
    tracing::info!("");
    tracing::info!("Architecture:");
    tracing::info!("  Command → Aggregate → Events → [event_store + outbox]");
    tracing::info!("                                         ↓");
    tracing::info!("                                    CDC Stream");
    tracing::info!("                                         ↓");
    tracing::info!("                          ┌──────────────┴──────────────┐");
    tracing::info!("                          ↓                             ↓");
    tracing::info!("                    Projections                    Redpanda");
    tracing::info!("                   (Read Models)                (External Systems)");
    tracing::info!("");
    tracing::info!("📊 Metrics available at: http://localhost:9090/metrics");
    tracing::info!("");

    Ok(())
}

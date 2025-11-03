use kameo::Actor;
use scylla::client::session::Session;
use scylla::client::session_builder::SessionBuilder;
use std::sync::Arc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use tokio::signal;

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
use event_sourcing::EventStore;
use domain::order::{OrderCommandHandler, OrderCommand, OrderItem, OrderEvent};
use domain::customer::{
    CustomerCommandHandler, CustomerCommand, CustomerEvent,
    Email, PhoneNumber, Address, CustomerTier,
};

#[tokio::main]
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
    let _coordinator = CoordinatorActor::spawn(CoordinatorActor::new(
        session.clone(),
        redpanda.clone(),
        metrics.clone()
    ));

    // === 5. Initialize Event Sourcing Components ===
    tracing::info!("🎯 Initializing Event Sourcing");

    // Create Order event store with metrics (generic EventStore<OrderEvent>)
    let event_store = Arc::new(EventStore::<OrderEvent>::with_metrics(
        session.clone(),
        "Order",         // aggregate type name
        "order-events",  // topic name
        metrics.clone()  // metrics for observability
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
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // === 7. Customer Event Sourcing Demo ===
    tracing::info!("");
    tracing::info!("════════════════════════════════════════════════════════════");
    tracing::info!("👤 Customer Event Sourcing Demo");
    tracing::info!("════════════════════════════════════════════════════════════");
    tracing::info!("");

    // Create Customer event store with metrics
    let customer_event_store = Arc::new(EventStore::<CustomerEvent>::with_metrics(
        session.clone(),
        "Customer",
        "customer-events",
        metrics.clone()
    ));

    let customer_command_handler = Arc::new(CustomerCommandHandler::new(customer_event_store.clone()));

    let customer_id = uuid::Uuid::new_v4();
    let customer_correlation_id = uuid::Uuid::new_v4();

    // Register Customer
    tracing::info!("1️⃣  Registering customer...");
    let version = customer_command_handler.handle(
        customer_id,
        CustomerCommand::RegisterCustomer {
            customer_id,
            email: Email::new("john.doe@example.com"),
            first_name: "John".to_string(),
            last_name: "Doe".to_string(),
            phone: Some(PhoneNumber::new("+1-555-0123")),
        },
        customer_correlation_id,
    ).await?;

    tracing::info!("   ✅ Customer registered: {} (version: {})", customer_id, version);
    tracing::info!("");

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Add Address
    tracing::info!("2️⃣  Adding customer address...");
    let address_id = uuid::Uuid::new_v4();
    let version = customer_command_handler.handle(
        customer_id,
        CustomerCommand::AddAddress {
            address_id,
            address: Address {
                street: "123 Main St".to_string(),
                city: "Springfield".to_string(),
                state: "IL".to_string(),
                postal_code: "62701".to_string(),
                country: "USA".to_string(),
            },
            set_as_default: true,
        },
        customer_correlation_id,
    ).await?;

    tracing::info!("   ✅ Address added (version: {})", version);
    tracing::info!("");

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Upgrade Tier
    tracing::info!("3️⃣  Upgrading customer tier...");
    let version = customer_command_handler.handle(
        customer_id,
        CustomerCommand::UpgradeTier {
            new_tier: CustomerTier::Gold,
        },
        customer_correlation_id,
    ).await?;

    tracing::info!("   ✅ Customer upgraded to Gold tier (version: {})", version);
    tracing::info!("");

    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    tracing::info!("");
    tracing::info!("════════════════════════════════════════════════════════════");
    tracing::info!("✅ Event Sourcing Demo Complete!");
    tracing::info!("════════════════════════════════════════════════════════════");
    tracing::info!("");
    tracing::info!("Aggregates Demonstrated:");
    tracing::info!("  📦 Order Aggregate:");
    tracing::info!("     - Created, Confirmed, Shipped, Delivered");
    tracing::info!("  👤 Customer Aggregate:");
    tracing::info!("     - Registered, Address Added, Tier Upgraded");
    tracing::info!("");
    tracing::info!("Event Sourcing Features:");
    tracing::info!("  ✅ Multiple domain aggregates (Order, Customer)");
    tracing::info!("  ✅ Generic event store infrastructure");
    tracing::info!("  ✅ Command handlers with business logic");
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
    tracing::info!("════════════════════════════════════════════════════════════");
    tracing::info!("🌐 Application is now running and ready!");
    tracing::info!("════════════════════════════════════════════════════════════");
    tracing::info!("");
    tracing::info!("📊 Available Endpoints:");
    tracing::info!("   Metrics:       http://localhost:9090/metrics");
    tracing::info!("   Health Check:  http://localhost:9090/health");
    tracing::info!("");
    tracing::info!("📈 Quick Metrics Check:");
    tracing::info!("   curl http://localhost:9090/metrics | grep cdc_processing_duration");
    tracing::info!("   curl http://localhost:9090/metrics | grep event_store_append");
    tracing::info!("");
    tracing::info!("💡 Tips:");
    tracing::info!("   - Run 'make metrics' to view current metrics");
    tracing::info!("   - See OBSERVABILITY.md for Prometheus/Grafana setup");
    tracing::info!("   - Press Ctrl+C to gracefully shutdown");
    tracing::info!("");
    tracing::info!("⏳ Waiting for shutdown signal...");

    // Wait for shutdown signal
    wait_for_shutdown().await;

    tracing::info!("");
    tracing::info!("🛑 Shutdown signal received, cleaning up...");

    // Give actors time to finish processing
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    tracing::info!("✅ Application stopped gracefully");

    Ok(())
}

/// Wait for shutdown signal (Ctrl+C or SIGTERM)
async fn wait_for_shutdown() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C signal");
        },
        _ = terminate => {
            tracing::info!("Received SIGTERM signal");
        },
    }
}

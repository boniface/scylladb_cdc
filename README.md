# ScyllaDB Event Sourcing with CDC

A production-ready Event Sourcing implementation using ScyllaDB CDC and Redpanda, built with Rust and the Actix actor model.

## What This Project Demonstrates

This project showcases **Event Sourcing** and **CQRS** patterns with CDC streaming:

- ✅ **Event Sourcing** - Events as source of truth
- ✅ **CQRS** - Separate write/read models
- ✅ **CDC Streaming** - Direct CDC consumption for projections
- ✅ **Outbox Pattern** - Reliable event publishing
- ✅ **Actor Supervision** - Fault-tolerant architecture
- ✅ **DLQ & Retry** - Production error handling
- ✅ **Prometheus Metrics** - Observability

## 📐 Architecture

```
Command → Aggregate → Events → [event_store + outbox] (atomic)
                                         ↓
                                    CDC Stream
                                         ↓
                          ┌──────────────┴──────────────┐
                          ↓                             ↓
                    Projections                    Redpanda
                   (Read Models)                (External Systems)
```

### Key Components

**Event Sourcing**:
- `EventStore` - Append-only event log (source of truth)
- `OrderAggregate` - Domain logic with business rules
- `OrderCommandHandler` - Validates commands, emits events
- `EventEnvelope` - Metadata (causation, correlation, versioning)

**CDC Streaming**:
- `CdcProcessor` - Streams from `outbox_messages` table
- Direct CDC consumption for projections (no polling)
- External publishing via Redpanda

**Infrastructure**:
- `CoordinatorActor` - Actor supervision tree
- `DlqActor` - Dead letter queue for failed messages
- `HealthCheckActor` - System health monitoring
- Prometheus metrics on `:9090/metrics`

## 🚀 Quick Start

### Prerequisites

- Rust 1.70+
- Docker & Docker Compose
- ScyllaDB (via Docker)
- Redpanda (via Docker)

### 1. Start Infrastructure

```bash
docker-compose up -d
```

This starts:
- ScyllaDB on `localhost:9042`
- Redpanda on `localhost:9092`

### 2. Initialize Schema

```bash
cqlsh -f src/db/schema.cql
```

Creates:
- `event_store` - Event log (source of truth)
- `outbox_messages` - CDC-enabled outbox (TTL 24h)
- `aggregate_sequence` - Optimistic locking
- `order_read_model` - Projection read model
- `orders_by_customer` - Projection by customer
- `orders_by_status` - Projection by status
- `dead_letter_queue` - Failed messages
- Snapshot and projection tracking tables

### 3. Run Application

```bash
cargo run
```

Watch the demo execute a complete order lifecycle:
1. **Create** order with items
2. **Confirm** order
3. **Ship** order with tracking
4. **Deliver** order with signature

Each command:
- Validates business rules
- Emits domain events
- Writes atomically to `event_store` + `outbox_messages`
- Streams via CDC to Redpanda

## 📊 Monitoring

- **Metrics**: http://localhost:9090/metrics
- **Redpanda Console**: http://localhost:8080 (if configured)
- **Logs**: Structured logging with tracing

## 🏗️ Project Structure

```
src/
├── event_sourcing/          # Event Sourcing core
│   ├── events.rs            # EventEnvelope + domain events
│   ├── aggregate.rs         # OrderAggregate with business logic
│   ├── event_store_simple.rs # EventStore + CommandHandler
│   └── projection_consumer.rs # Direct CDC projections
├── actors/                  # Actor system
│   ├── coordinator.rs       # Supervision tree
│   ├── cdc_processor.rs     # CDC streaming
│   ├── dlq_actor.rs         # Dead letter queue
│   └── health_check.rs      # Health monitoring
├── messaging/               # Redpanda integration
│   └── redpanda.rs          # Kafka client
├── utils/                   # Infrastructure
│   ├── circuit_breaker.rs   # Circuit breaker pattern
│   └── retry.rs             # Exponential backoff
├── metrics/                 # Prometheus metrics
└── main.rs                  # Application entry point
```

## 🎓 Event Sourcing Concepts

### Commands
User intentions that MAY succeed:
```rust
OrderCommand::CreateOrder { customer_id, items }
OrderCommand::ConfirmOrder
OrderCommand::ShipOrder { tracking_number, carrier }
```

### Events
Facts that DID happen (immutable):
```rust
OrderEvent::Created { customer_id, items }
OrderEvent::Confirmed { confirmed_at }
OrderEvent::Shipped { tracking_number, carrier, shipped_at }
```

### Aggregate
Domain model that:
- Validates commands
- Emits events
- Rebuilds state from events
- Enforces business rules

```rust
// Command validation
if order.status != OrderStatus::Confirmed {
    return Err(OrderError::NotConfirmed);
}
```

### Event Store
Append-only log:
- Events NEVER deleted or modified
- Current state = replay all events
- Full audit trail
- Time travel debugging

### Projections
Read models built from events:
- `order_read_model` - Current order state
- `orders_by_customer` - Customer's orders
- `orders_by_status` - Operational dashboards
- Can be rebuilt at any time

## 🔄 CDC Architecture

### Direct CDC Consumption (Approach 2)

**For Internal Projections**:
```
event_store + outbox → CDC Stream → Projection Consumer → Read Model
                                    ↑
                              Direct consumption
                              (no intermediary)
```

**For External Systems**:
```
event_store + outbox → CDC Stream → Redpanda Publisher → Redpanda
                                    ↑
                              One of many CDC consumers
```

**Benefits**:
- ✅ Low latency (~50ms)
- ✅ Independent consumers
- ✅ Fault isolation
- ✅ Simple architecture

See `CDC_PROJECTIONS_ARCHITECTURE.md` for deep dive.

## ✨ Event Sourcing Features

### Implemented ✅

- [x] Event Store (append-only)
- [x] Aggregate Root pattern
- [x] Command Handler with validation
- [x] Event metadata (causation, correlation, versioning)
- [x] Optimistic concurrency control
- [x] Atomic write to event_store + outbox
- [x] CDC streaming to Redpanda
- [x] DLQ for failed messages
- [x] Retry with exponential backoff
- [x] Circuit breaker
- [x] Prometheus metrics
- [x] Actor supervision

### Ready to Implement 🚧

- [ ] Projections (code exists, needs concrete implementations)
- [ ] Snapshots (schema ready, needs integration)
- [ ] Event replay (for rebuilding projections)
- [ ] Event upcasting (schema versioning)
- [ ] Sagas (multi-aggregate transactions)

## 📝 Usage Example

```rust
use event_sourcing::{EventStore, OrderCommandHandler, OrderCommand, OrderItem};

// Initialize
let event_store = Arc::new(EventStore::new(session));
let handler = Arc::new(OrderCommandHandler::new(event_store));

// Execute command
let version = handler.handle(
    order_id,
    OrderCommand::CreateOrder {
        order_id,
        customer_id,
        items: vec![OrderItem { product_id, quantity: 2 }],
    },
    correlation_id,
).await?;

// Events are now in:
// - event_store (permanent)
// - outbox_messages (CDC streams this)

// CDC will stream to:
// - Projections (update read models)
// - Redpanda (external systems)
```

## 🧪 Testing

```bash
# Run tests
cargo test

# Run with logs
RUST_LOG=debug cargo test

# Test specific module
cargo test event_sourcing::aggregate::tests
```

Tests cover:
- Aggregate lifecycle
- Business rule enforcement
- Event application
- Status transitions
- Concurrency conflicts

## 📚 Documentation

Comprehensive documentation is available in the [`docs/`](./docs/) folder:

- **[Quick Start Guide](./docs/QUICKSTART.md)** - Get up and running in 5 minutes
- **[Event Sourcing Guide](./docs/EVENT_SOURCING_GUIDE.md)** - Core concepts and patterns
- **[Architecture](./docs/ARCHITECTURE.md)** - System design and CDC implementation
- **[Architecture (AsciiDoc)](./docs/ARCHITECTURE.adoc)** - System design with C4 diagrams (PlantUML)
- **[Code Walkthrough](./docs/CODE_WALKTHROUGH.md)** - Detailed code structure
- **[Diagrams](./docs/DIAGRAMS.md)** - Visual architecture diagrams
- **[FAQ](./docs/FAQ.md)** - Frequently asked questions
- **[Test Audit](./docs/TEST_AUDIT.md)** - Test coverage status and roadmap
- **[Contributing](./docs/CONTRIBUTING.md)** - Contribution guidelines

Additional reference: [`src/db/schema.cql`](./src/db/schema.cql) - Annotated database schema

## 🔧 Configuration

### Environment Variables

```bash
RUST_LOG=info                    # Log level
SCYLLA_NODES=127.0.0.1:9042     # ScyllaDB contact points
REDPANDA_BROKERS=127.0.0.1:9092 # Redpanda brokers
METRICS_PORT=9090                # Prometheus metrics port
```

### docker-compose.yml

Customize:
- ScyllaDB memory limits
- Redpanda configuration
- Port mappings

## 🐛 Troubleshooting

### Schema Not Found

```bash
cqlsh -f src/db/schema.cql
```

### Connection Refused

Check services are running:
```bash
docker-compose ps
```

### Concurrency Conflicts

Normal for event sourcing - command handler will retry.

### CDC Not Streaming

Verify CDC is enabled:
```sql
DESC TABLE orders_ks.outbox_messages;
-- Should show: cdc = {'enabled': true}
```

## 🎯 Production Considerations

### Performance

Current implementation uses single inserts for clarity. For production:

**Use Batches**:
```rust
let mut batch = Batch::default();
batch.append_statement("INSERT INTO event_store ...");
batch.append_statement("INSERT INTO outbox_messages ...");
session.batch(&batch, values).await?;
```

### Scalability

- **Horizontal**: Add more ScyllaDB nodes
- **Partitioning**: Aggregate ID is partition key
- **Snapshots**: Every 100 events (schema ready)
- **Read models**: Independent scaling per projection

### Monitoring

Watch:
- Event store write latency
- CDC consumer lag
- DLQ message count
- Circuit breaker state
- Projection lag

## 📖 References

- [Event Sourcing by Martin Fowler](https://martinfowler.com/eaaDev/EventSourcing.html)
- [CQRS by Greg Young](https://cqrs.files.wordpress.com/2010/11/cqrs_documents.pdf)
- [ScyllaDB CDC](https://docs.scylladb.com/stable/using-scylla/cdc/)
- [Outbox Pattern](https://microservices.io/patterns/data/transactional-outbox.html)

## 🤝 Contributing

This is an educational project. Contributions welcome:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests
5. Submit a pull request

## 📄 License

MIT License - see LICENSE file for details

---

**Built with** 🦀 Rust + ⚡ ScyllaDB + 🐼 Redpanda

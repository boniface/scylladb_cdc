# ScyllaDB CDC Outbox Pattern Demo

An educational demonstration of the **Transactional Outbox Pattern** using ScyllaDB CDC and Redpanda, implemented with Rust and the Actix actor model.

## 🎯 What This Project Demonstrates

This project shows how to reliably publish domain events from a database to a message queue without dual-write problems, using:

- **ScyllaDB** - High-performance NoSQL database
- **Change Data Capture (CDC)** - Stream database changes
- **Redpanda** - Kafka-compatible message broker
- **Actix** - Actor-based concurrency model in Rust
- **Transactional Outbox Pattern** - Reliable event publishing

## 📚 The Outbox Pattern Explained

### The Problem: Dual Writes

Traditional approaches that write to both a database AND a message queue face the **dual-write problem**:

```rust
// ❌ PROBLEM: What if one succeeds and the other fails?
db.save(order).await?;           // Write 1
message_queue.publish(event).await?;  // Write 2
```

**Issues:**
- If the DB write succeeds but message publish fails → Event is lost
- If the message publish succeeds but DB write fails → Inconsistent state
- No way to ensure both operations succeed atomically

### The Solution: Transactional Outbox

The outbox pattern solves this by:

1. **Writing both the domain data AND events in a single transaction**
   ```rust
   // ✅ SOLUTION: Single atomic batch write
   batch.append("INSERT INTO orders ...");
   batch.append("INSERT INTO outbox_messages ...");
   db.batch(batch).await?;  // Both succeed or both fail
   ```

2. **Reading events from the outbox asynchronously**
   - A separate process (CDC processor) reads from the outbox table
   - Publishes events to the message queue
   - Tracks progress to handle restarts

3. **Using CDC for real-time streaming** (Phase 3)
   - ScyllaDB CDC automatically captures changes
   - Low latency, no polling overhead
   - Built-in ordering guarantees

## 🏗️ Architecture (Phase 5: Production Ready)

```
┌───────────────────────────────────────────────────────┐
│        CoordinatorActor (Supervisor)                  │
│  - Manages child actor lifecycle                     │
│  - Handles graceful shutdown                         │
│  - Coordinates system health                         │
└────┬─────────┬──────────┬──────────┬────────────────┘
     │         │          │          │
     ▼         ▼          ▼          ▼
┌─────────┐ ┌────┐ ┌──────────┐ ┌──────────────┐
│ Order   │ │DLQ │ │   CDC    │ │ HealthCheck  │
│ Actor   │ │    │ │ Stream   │ │    Actor     │
└────┬────┘ │    │ │Processor │ └──────────────┘
     │      └─┬──┘ └─────┬────┘       │
     │        │          │            │ Monitors
     │Batch   │Failed    │ ┌──────────▼────────┐
     │Write   │Events    │ │  Retry + Metrics  │
     ├────────┼──────────┤ └───────────────────┘
     ▼        ▼          │            │
┌─────────┐ ┌──────────────┐         │
│ orders  │ │dead_letter_  │         │
│  table  │ │    queue     │         │
└─────────┘ └──────────────┘         │
            ▼                         │
    ┌──────────────┐           ┌─────▼────────┐
    │outbox_messages│           │ Circuit      │
    │ (CDC enabled) │           │ Breaker      │
    └───────┬───────┘           └──────┬───────┘
            │                          │
       CDC Stream                      │ Protects
            │                          │
            ▼                          ▼
      ┌──────────────────────────────────┐
      │       Redpanda/Kafka             │
      └──────────────────────────────────┘
               │
               ▼
      ┌──────────────────────────────────┐
      │   Prometheus Metrics (9090)      │
      │  - CDC events processed          │
      │  - Retry attempts                │
      │  - DLQ messages                  │
      │  - Circuit breaker state         │
      └──────────────────────────────────┘
```

## 🚀 Getting Started

### Prerequisites

- Rust (latest stable)
- Docker & Docker Compose
- cqlsh (for manual database inspection)

### Quick Start (Using Makefile)

```bash
# Start everything (services + schema + app)
make dev

# View Prometheus metrics
make metrics

# Run integration tests
make integration-test

# Clean up
make clean
```

### Manual Setup

#### 1. Start Infrastructure

```bash
# Start ScyllaDB and Redpanda
docker-compose up -d

# Wait for ScyllaDB to be ready (about 30 seconds)
docker-compose logs -f scylla
```

#### 2. Initialize Database

```bash
# Use Makefile
make schema

# Or manually:
cqlsh -f src/db/schema.cql
cqlsh -f src/db/dlq_schema.cql
```

#### 3. Run the Application

```bash
# Run with default logging (INFO level)
cargo run

# Run with debug logging to see all details
RUST_LOG=debug cargo run

# Build release version
cargo build --release
```

### 4. Observe the Flow

The application will:
1. Create an order → writes to `orders` + `outbox_messages` atomically
2. CDC Stream Processor captures change in real-time
3. Retry logic handles transient failures (up to 5 attempts)
4. Failed messages go to Dead Letter Queue
5. Metrics exposed at `http://localhost:9090/metrics`

Watch the logs to see each step:

```
🚀 Starting ScyllaDB CDC Outbox Pattern Demo
📡 Phase 3: Real CDC Streams
🎯 Phase 4: Actor Supervision & Circuit Breaker
📊 Phase 5: DLQ, Retry, & Metrics
🎯 CoordinatorActor started - Phase 4: Actor Supervision
✅ All supervised actors started successfully
🔄 Starting CDC streaming for outbox_messages table
📤 Publishing event from CDC stream to Redpanda
✅ Successfully published event via CDC stream
```

### 5. Monitor with Metrics

```bash
# View all metrics
curl http://localhost:9090/metrics

# Or use Makefile
make metrics

# Check health endpoint
curl http://localhost:9090/health
```

**Key Metrics to Watch:**
- `cdc_events_processed_total` - Total events processed
- `retry_attempts_total` - Retry attempts by operation
- `dlq_messages_total` - Messages in Dead Letter Queue
- `circuit_breaker_state` - Circuit breaker status

## 📖 Code Structure

```
src/
├── main.rs                           # Application entry point (uses coordinator)
├── models.rs                         # Domain models and events
├── actors/
│   ├── coordinator.rs                # Phase 4: Supervision & orchestration
│   ├── health_check.rs               # Phase 4: System health monitoring
│   ├── dlq_actor.rs                  # Phase 5: Dead Letter Queue
│   ├── order_actor.rs                # Handles order commands with transactional outbox
│   ├── cdc_stream_processor.rs       # Phase 3: Real CDC streams (ACTIVE)
│   └── cdc_processor_polling.rs      # Phase 2: Polling approach (reference)
├── messaging/
│   └── redpanda.rs                   # Kafka/Redpanda client (with circuit breaker)
├── utils/
│   ├── circuit_breaker.rs            # Phase 4: Circuit breaker pattern
│   ├── retry.rs                      # Phase 5: Retry with exponential backoff
│   └── mod.rs                        # Utilities module
├── metrics/
│   ├── mod.rs                        # Phase 5: Prometheus metrics
│   └── server.rs                     # Metrics HTTP server
└── db/
    ├── schema.cql                    # Database schema with CDC enabled
    └── dlq_schema.cql                # Phase 5: Dead Letter Queue schema

tests/
└── integration_test.sh               # Phase 5: Full integration tests

Makefile                              # Phase 5: Development commands
```

## 🔍 Key Concepts Demonstrated

### 1. Transactional Writes (order_actor.rs:47-99)

The `persist_with_outbox` function uses ScyllaDB batches to ensure atomicity:

```rust
let mut batch = Batch::default();
batch.append_statement(order_query);
batch.append_statement("INSERT INTO outbox_messages ...");

session.batch(&batch, values).await?;
```

### 2. Real CDC Streams (cdc_stream_processor.rs) ⭐ NEW

The application now uses **real ScyllaDB CDC streams** via the `scylla-cdc` library:

```rust
// Implement Consumer trait to process CDC rows
#[async_trait]
impl Consumer for OutboxCDCConsumer {
    async fn consume_cdc(&mut self, data: CDCRow<'_>) -> anyhow::Result<()> {
        // Extract event from CDC row
        let event = extract_event_from_cdc_row(&data)?;
        // Publish to Redpanda
        self.redpanda.publish(&event.event_type, &event.payload).await?;
        Ok(())
    }
}

// Start the CDC log reader
CDCLogReaderBuilder::new()
    .session(session)
    .keyspace("orders_ks")
    .table_name("outbox_messages")
    .consumer_factory(factory)
    .build()
    .await?
```

**Benefits over polling:**
- **Zero polling overhead** - events arrive via push, not pull
- **Lower latency** - near real-time delivery
- **Automatic generation handling** - library manages CDC topology changes
- **Built-in checkpointing** - resume from last position automatically

### 3. Idempotency (Phase 2 approach in cdc_processor_polling.rs)

Events are tracked to prevent duplicate processing:

```rust
if processed_ids.contains(&msg.id) {
    continue;  // Skip already processed
}
```

### 4. Actor Supervision (coordinator.rs) ⭐ Phase 4

Production-grade actor supervision with hierarchy:

```rust
// Coordinator supervises all child actors
pub struct CoordinatorActor {
    session: Arc<Session>,
    redpanda: Arc<RedpandaClient>,
    order_actor: Option<Addr<OrderActor>>,
    cdc_processor: Option<Addr<CdcStreamProcessor>>,
    health_check: Option<Addr<HealthCheckActor>>,
}

// Graceful shutdown
impl Handler<Shutdown> for CoordinatorActor {
    fn handle(&mut self, _msg: Shutdown, ctx: &mut Self::Context) {
        // Stop all child actors gracefully
        self.order_actor.do_send(StopActor);
        self.cdc_processor.do_send(StopActor);
        ctx.stop();
    }
}
```

**Benefits:**
- **Fault isolation** - Failure in one actor doesn't crash others
- **Lifecycle management** - Coordinated start/stop
- **Graceful shutdown** - Clean resource cleanup
- **Health monitoring** - Track system status

### 5. Circuit Breaker (circuit_breaker.rs) ⭐ Phase 4

Prevents cascading failures to Redpanda:

```rust
pub enum CircuitState {
    Closed,     // Normal operation
    Open,       // Blocking requests (after 5 failures)
    HalfOpen,   // Testing recovery
}

// Automatically applied in RedpandaClient
circuit_breaker.call(async {
    producer.send(record).await
}).await?;
```

**Failure Protection:**
- Opens after 5 consecutive failures
- Blocks requests while open (30s timeout)
- Tests recovery in half-open state
- Closes after 3 successful requests

## 🧪 Exploring Further

### Inspect the Database

```bash
# Connect to ScyllaDB
docker exec -it $(docker ps -qf "name=scylla") cqlsh

# View orders
USE orders_ks;
SELECT * FROM orders;

# View outbox messages
SELECT * FROM outbox_messages;

# View CDC offsets
SELECT * FROM cdc_offsets;
```

### Consume from Redpanda

```bash
# Install rpk (Redpanda CLI)
# Then consume from the event topics:

rpk topic consume OrderCreated --brokers localhost:9092
rpk topic consume OrderUpdated --brokers localhost:9092
rpk topic consume OrderCancelled --brokers localhost:9092
```

### Test Resilience

```bash
# Stop the app mid-execution (Ctrl+C)
# Restart it - CDC processor will resume from last offset
cargo run
```

## 📈 Development Phases

### ✅ Phase 1 & 2: Foundation (COMPLETE)
- [x] Fixed configuration and schema
- [x] Transactional batched writes
- [x] Polling-based CDC processor (educational)
- [x] Offset tracking and idempotency
- [x] Full event lifecycle (Create/Update/Cancel)

### ✅ Phase 3: Real CDC Streams (COMPLETE)
- [x] Integrate `scylla-cdc` library
- [x] Stream CDC log tables in real-time
- [x] Handle CDC generations automatically
- [x] Near real-time event delivery
- [x] Proper ordering guarantees

**Status:** The application uses **real ScyllaDB CDC streams** via the `scylla-cdc` library. This provides:
- **True streaming** (no polling)
- **Low latency** (events arrive as written)
- **Automatic generation handling**
- **Built-in checkpointing**

The old polling implementation is preserved at `src/actors/cdc_processor_polling.rs` for educational comparison.

### ✅ Phase 4: Actor Refinement (COMPLETE)
- [x] Coordinator actor for supervision
- [x] Health check actor for monitoring
- [x] Circuit breaker pattern for Redpanda
- [x] Graceful shutdown handling
- [x] Actor hierarchy and supervision

**Status:** The application features **production-grade actor supervision** with:
- **CoordinatorActor** - Supervises all child actors
- **HealthCheckActor** - Monitors component health
- **Circuit Breaker** - Protects against Redpanda failures
- **Graceful Shutdown** - Clean actor termination
- **Actor Hierarchy** - Proper supervision tree

### ✅ Phase 5: Production Readiness (COMPLETE)
- [x] Dead Letter Queue (DLQ) for failed messages
- [x] Retry mechanism with exponential backoff
- [x] Prometheus metrics and observability
- [x] Integration tests with docker-compose
- [x] Developer-friendly Makefile
- [x] Production-ready error handling

**Status:** The application is now **production-ready** with:
- **Dead Letter Queue** - Persistent storage of failed messages
- **Retry Logic** - 5 attempts with exponential backoff (100ms → 500ms)
- **Metrics** - Comprehensive Prometheus metrics at `:9090/metrics`
- **Integration Tests** - Automated end-to-end testing
- **Observability** - Full system monitoring and health checks

The system handles failures gracefully, provides complete observability, and includes automated testing!

### ✅ Phase 6: Educational Documentation (COMPLETE - Current Implementation)
- [x] Interactive tutorial for beginners
- [x] Detailed code walkthroughs with annotations
- [x] Visual diagrams for key concepts
- [x] Comprehensive FAQ and troubleshooting guide
- [x] Video walkthrough script
- [x] Complete documentation index

**Current Status:** The project now includes **world-class educational materials**:
- **[Interactive Tutorial](./docs/TUTORIAL.md)** - 800+ lines of hands-on learning
- **[Code Walkthrough](./docs/CODE_WALKTHROUGH.md)** - 1,290+ lines of detailed explanations
- **[Visual Diagrams](./docs/DIAGRAMS.md)** - 1,000+ lines of ASCII architecture diagrams
- **[FAQ](./docs/FAQ.md)** - 700+ lines of Q&A and troubleshooting
- **[Video Script](./docs/VIDEO_SCRIPT.md)** - 500+ lines for creating video tutorials
- **[Documentation Index](./docs/INDEX.md)** - Navigation hub with learning paths

**Total: 6,000+ lines of comprehensive educational content** across 11 documents!

## 🎓 Learning Resources

### Project Documentation

**Getting Started:**
- **[README.md](./README.md)** - You are here! Project overview
- **[QUICKSTART.md](./QUICKSTART.md)** - Get running in 3 commands
- **[docs/INDEX.md](./docs/INDEX.md)** - Documentation navigation hub

**Learning Materials:**
- **[docs/TUTORIAL.md](./docs/TUTORIAL.md)** - Interactive hands-on tutorial (60-90 min)
- **[docs/CODE_WALKTHROUGH.md](./docs/CODE_WALKTHROUGH.md)** - Line-by-line code explanations
- **[docs/DIAGRAMS.md](./docs/DIAGRAMS.md)** - Visual architecture and flows
- **[docs/FAQ.md](./docs/FAQ.md)** - Common questions and troubleshooting
- **[docs/VIDEO_SCRIPT.md](./docs/VIDEO_SCRIPT.md)** - Video tutorial script

**Technical Deep Dives:**
- **[COMPARISON.md](./COMPARISON.md)** - Polling vs CDC streaming analysis
- **[PHASE3_CHANGES.md](./PHASE3_CHANGES.md)** - Real CDC streams implementation
- **[PHASE4_CHANGES.md](./PHASE4_CHANGES.md)** - Actor supervision and circuit breaker
- **[PHASE5_CHANGES.md](./PHASE5_CHANGES.md)** - DLQ, retry, and metrics
- **[PHASE6_EDUCATIONAL.md](./PHASE6_EDUCATIONAL.md)** - Educational documentation overview

### Outbox Pattern
- [Microservices.io - Transactional Outbox](https://microservices.io/patterns/data/transactional-outbox.html)
- [Debezium Outbox Pattern](https://debezium.io/blog/2019/02/19/reliable-microservices-data-exchange-with-the-outbox-pattern/)

### ScyllaDB CDC
- [ScyllaDB CDC Documentation](https://docs.scylladb.com/stable/using-scylla/cdc/)
- [scylla-cdc-rust Library](https://github.com/scylladb/scylla-cdc-rust)
- [CDC Tutorial](https://github.com/scylladb/scylla-cdc-rust/blob/main/tutorial.md)

### Actor Model
- [Actix Documentation](https://actix.rs/)
- [The Actor Model](https://en.wikipedia.org/wiki/Actor_model)

## 🐛 Troubleshooting

### ScyllaDB won't start
```bash
# Check logs
docker-compose logs scylla

# Try increasing Docker memory (needs at least 2GB)
```

### "ALLOW FILTERING" warnings
The old polling implementation (Phase 2, preserved for reference) uses ALLOW FILTERING. The current CDC streaming implementation (Phase 3) doesn't use polling at all.

### Events not appearing in Redpanda
```bash
# Check CDC processor is running
docker-compose logs app | grep "CdcProcessor"

# Verify outbox has events
cqlsh -e "SELECT * FROM orders_ks.outbox_messages;"
```

## 📝 License

MIT - This is an educational project

## 🙏 Contributing

This is an educational demo. Feel free to fork and experiment!

## 📧 Questions?

Open an issue or check the extensive inline documentation in the code.

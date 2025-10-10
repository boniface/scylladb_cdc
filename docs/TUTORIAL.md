# Interactive Tutorial: Building a CDC Outbox Pattern System

Welcome! This tutorial will guide you through understanding and building the ScyllaDB CDC Outbox Pattern step by step.

## 📚 Table of Contents

1. [Prerequisites](#prerequisites)
2. [Understanding the Problem](#understanding-the-problem)
3. [Hands-On: Part 1 - Basic Outbox](#hands-on-part-1---basic-outbox)
4. [Hands-On: Part 2 - CDC Streaming](#hands-on-part-2---cdc-streaming)
5. [Hands-On: Part 3 - Production Features](#hands-on-part-3---production-features)
6. [Testing Your Implementation](#testing-your-implementation)
7. [Common Pitfalls](#common-pitfalls)
8. [Next Steps](#next-steps)

---

## Prerequisites

Before starting, ensure you have:

- ✅ Rust installed (1.70+)
- ✅ Docker and Docker Compose
- ✅ Basic understanding of databases and message queues
- ✅ Familiarity with async Rust (not required, but helpful)

**Time Required**: 60-90 minutes

---

## Understanding the Problem

### The Dual-Write Problem

Imagine you're building an e-commerce system. When an order is created, you need to:

1. **Save the order to the database** (for persistence)
2. **Publish an event to a message queue** (for other services to react)

**Naive Approach:**
```rust
// ❌ PROBLEM: What if one succeeds and the other fails?
async fn create_order(order: Order) -> Result<()> {
    database.save(&order).await?;        // Write 1
    message_queue.publish(event).await?; // Write 2
    Ok(())
}
```

**What can go wrong?**

| Scenario | Database | Message Queue | Result |
|----------|----------|---------------|--------|
| Happy Path | ✅ Saved | ✅ Published | Perfect! |
| DB Fails | ❌ Failed | ❌ Skipped | Consistent (no data) |
| MQ Fails | ✅ Saved | ❌ Failed | **INCONSISTENT** 😱 |
| Network Issue | ✅ Saved | ⏳ Timeout | **UNKNOWN STATE** 😱 |

The problem: **You can't guarantee both operations succeed atomically.**

### The Solution: Transactional Outbox Pattern

Instead of two separate writes, we:

1. **Write BOTH the order and the event in a single transaction**
2. **A separate process reads the events and publishes them**

```rust
// ✅ SOLUTION: Single atomic transaction
async fn create_order(order: Order) -> Result<()> {
    let mut batch = Batch::new();
    batch.append("INSERT INTO orders ...");
    batch.append("INSERT INTO outbox_messages ...");

    database.batch(batch).await?; // Both succeed or both fail
    Ok(())
}
```

Now we have a **new problem**: How does the separate process know about new events?

**Two approaches:**

1. **Polling** (Phase 2): Query the outbox table periodically
   - ✅ Simple to implement
   - ❌ Wasteful (queries even when nothing changes)
   - ❌ Higher latency

2. **CDC Streaming** (Phase 3): Database pushes changes
   - ✅ Near real-time (< 100ms)
   - ✅ Efficient (no polling)
   - ✅ Guaranteed ordering

This project demonstrates **BOTH** approaches for educational purposes!

---

## Hands-On: Part 1 - Basic Outbox

Let's start by implementing the basic transactional outbox pattern.

### Step 1: Understand the Schema

Open `src/db/schema.cql` and examine:

```sql
-- Your business data
CREATE TABLE orders (
    id UUID PRIMARY KEY,
    customer_id UUID,
    items TEXT,
    status TEXT,
    created_at TIMESTAMP,
    updated_at TIMESTAMP
);

-- The outbox table (stores events)
CREATE TABLE outbox_messages (
    id UUID PRIMARY KEY,
    aggregate_id UUID,        -- References the order
    event_type TEXT,          -- e.g., "OrderCreated"
    payload TEXT,             -- JSON event data
    created_at TIMESTAMP
) WITH cdc = {'enabled': true}; -- CDC enabled for streaming!
```

**Key Insight**: Both tables are in the same keyspace, allowing atomic batches.

### Step 2: Explore the Transactional Write

Open `src/actors/order_actor.rs` and find the `persist_with_outbox` function:

```rust
async fn persist_with_outbox<E: DomainEvent + serde::Serialize>(
    &self,
    order_query: &str,
    order_values: impl SerializeRow,
    event: &E,
) -> anyhow::Result<()> {
    // 1. Serialize the event
    let event_id = Uuid::new_v4();
    let payload = serde_json::to_string(event)?;
    let timestamp = Utc::now();

    // 2. Create a batch with BOTH writes
    let mut batch = Batch::default();
    batch.append_statement(order_query);
    batch.append_statement(
        "INSERT INTO outbox_messages (id, aggregate_id, event_type, payload, created_at)
         VALUES (?, ?, ?, ?, ?)"
    );

    // 3. Execute atomically - both succeed or both fail
    self.session.batch(
        &batch,
        (
            order_values,
            (event_id, event.aggregate_id(), event.event_type(), payload, timestamp)
        )
    ).await?;

    tracing::info!(
        order_id = %event.aggregate_id(),
        event_type = %event.event_type(),
        "✅ Transactionally persisted order and outbox event"
    );

    Ok(())
}
```

**Exercise 1**: Run the application and observe the logs:

```bash
make dev
```

Look for these log lines:
```
✅ Transactionally persisted order and outbox event
```

**Question**: What happens if Redpanda is down? Does the order still get saved?

<details>
<summary>Click to reveal answer</summary>

**Yes!** The order and the outbox event are both saved to ScyllaDB. The CDC processor will retry publishing to Redpanda later. This is the beauty of the outbox pattern - your business logic is decoupled from message queue availability.
</details>

### Step 3: Verify the Data

```bash
# Connect to ScyllaDB
docker exec -it $(docker-compose ps -q scylla) cqlsh

# View orders
USE orders_ks;
SELECT * FROM orders;

# View outbox messages
SELECT id, event_type, aggregate_id, created_at FROM outbox_messages;
```

You should see 3 events: OrderCreated, OrderUpdated, OrderCancelled.

**Exercise 2**: Create a new order and watch it appear in both tables.

---

## Hands-On: Part 2 - CDC Streaming

Now let's see how CDC automatically captures changes.

### Step 1: Understand CDC Architecture

When you enable CDC on a table:

```sql
CREATE TABLE outbox_messages (...)
WITH cdc = {'enabled': true, 'postimage': true};
```

ScyllaDB automatically:
1. Creates hidden log tables (you don't see them)
2. Writes changes to these log tables
3. Organizes them by "streams" for parallel processing

### Step 2: Explore the CDC Consumer

Open `src/actors/cdc_processor.rs` and find the `OutboxCDCConsumer`:

```rust
#[async_trait]
impl Consumer for OutboxCDCConsumer {
    async fn consume_cdc(&mut self, data: CDCRow<'_>) -> anyhow::Result<()> {
        tracing::debug!(
            stream_id = ?data.stream_id,
            operation = %data.operation,
            "Received CDC row"
        );

        // 1. Extract event from CDC row
        match self.extract_event_from_cdc_row(&data)? {
            Some(event) => {
                // 2. Publish with retry
                let result = retry_with_backoff(
                    self.retry_config.clone(),
                    |attempt| {
                        let redpanda = self.redpanda.clone();
                        let event_type = event.event_type.clone();
                        let payload = event.payload.clone();

                        async move {
                            redpanda.publish(&event_type, &event_id, &payload).await
                        }
                    }
                ).await;

                // 3. Handle success or send to DLQ
                match result {
                    RetryResult::Success(_) => {
                        tracing::info!("✅ Successfully published event via CDC stream");
                    }
                    RetryResult::Failed(e) => {
                        tracing::error!("❌ Failed after retries, sending to DLQ");
                        self.dlq_actor.do_send(AddToDlq { ... });
                    }
                }
            }
            None => Ok(()) // Non-insert operation, skip
        }
    }
}
```

**Key Concepts:**

1. **Consumer Trait**: Your code implements this to process each CDC row
2. **CDCRow**: Contains the actual data that was inserted/updated
3. **Operation Type**: INSERT, UPDATE, DELETE, etc.
4. **Stream ID**: Used for parallel processing

### Step 3: Watch CDC in Action

```bash
# Terminal 1: Start the app with debug logs
RUST_LOG=debug make dev

# Terminal 2: Monitor CDC logs
docker-compose logs -f app | grep CDC
```

You should see:
```
🔄 Starting CDC streaming for outbox_messages table
Received CDC row stream_id=... operation=RowInsert
📤 Publishing event from CDC stream to Redpanda
✅ Successfully published event via CDC stream
```

**Exercise 3**: Stop Redpanda and create an order. Watch the retry mechanism:

```bash
# Stop Redpanda
docker-compose stop redpanda

# Restart the app (it will create test orders)
cargo run

# Observe retry logs
```

You'll see:
```
Attempt 1/5 - Attempting to publish event
Attempt 2/5 - Attempting to publish event (after 100ms)
Attempt 3/5 - Attempting to publish event (after 200ms)
...
❌ Failed to publish event after retries, sending to DLQ
💀 Adding message to Dead Letter Queue
```

### Step 4: Compare with Polling Approach

Open `src/actors/cdc_processor_polling.rs` (preserved for education):

```rust
// Polling approach - queries every 5 seconds
ctx.run_interval(Duration::from_secs(5), |act, _ctx| {
    act.fetch_and_publish();
});

async fn fetch_and_publish(&mut self) {
    // Query for new messages
    let result = self.session.query_unpaged(
        "SELECT id, aggregate_id, event_type, payload, created_at
         FROM outbox_messages
         WHERE created_at > ?
         ALLOW FILTERING",
        (self.last_offset,)
    ).await?;

    // Process messages...
}
```

**Comparison:**

| Feature | Polling | CDC Streaming |
|---------|---------|---------------|
| Latency | 0-5 seconds | < 100ms |
| Database Load | Query every 5s | No queries |
| Scalability | Limited | Excellent |
| Complexity | Simple | Moderate |
| Ordering | Manual | Guaranteed |

**Exercise 4**: Read `COMPARISON.md` for detailed performance analysis.

---

## Hands-On: Part 3 - Production Features

Now let's explore the production-ready features.

### Step 1: Understanding Retry Logic

Open `src/utils/retry.rs`:

```rust
pub struct RetryConfig {
    pub max_attempts: u32,      // How many times to retry
    pub initial_delay: Duration, // First retry delay
    pub max_delay: Duration,     // Cap on delay
    pub multiplier: f64,         // Exponential factor
}

// Aggressive preset (used for CDC events)
pub fn aggressive() -> Self {
    Self {
        max_attempts: 5,
        initial_delay: Duration::from_millis(100),
        max_delay: Duration::from_millis(500),
        multiplier: 2.0,
    }
}
```

**Retry Timeline:**
```
Attempt 1: Immediate
   ↓ (fail)
Wait 100ms
   ↓
Attempt 2: After 100ms
   ↓ (fail)
Wait 200ms (100ms * 2.0)
   ↓
Attempt 3: After 200ms
   ↓ (fail)
Wait 400ms (200ms * 2.0)
   ↓
Attempt 4: After 400ms
   ↓ (fail)
Wait 500ms (capped at max_delay)
   ↓
Attempt 5: After 500ms
   ↓ (fail)
→ Send to DLQ
```

**Exercise 5**: Modify the retry config to be more conservative:

```rust
// In src/actors/cdc_processor.rs
retry_config: RetryConfig::conservative(), // Instead of aggressive()
```

Rebuild and observe the longer delays:
```bash
cargo build && cargo run
```

### Step 2: Dead Letter Queue (DLQ)

Open `src/actors/dlq_actor.rs`:

```rust
impl Handler<AddToDlq> for DlqActor {
    type Result = ResponseFuture<Result<(), String>>;

    fn handle(&mut self, msg: AddToDlq, _: &mut Self::Context) -> Self::Result {
        tracing::error!(
            event_id = %msg.id,
            event_type = %msg.event_type,
            error = %msg.error_message,
            failure_count = msg.failure_count,
            "💀 Adding message to Dead Letter Queue"
        );

        // Persist to dead_letter_queue table
        session.query_unpaged(
            "INSERT INTO dead_letter_queue
             (id, aggregate_id, event_type, payload, error_message,
              failure_count, first_failed_at, last_failed_at, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (msg.id, msg.aggregate_id, ...)
        ).await?;
    }
}
```

**Exercise 6**: Force a message to DLQ and query it:

```bash
# Stop Redpanda to force failures
docker-compose stop redpanda

# Run the app (messages will fail after 5 retries)
cargo run

# Wait 30 seconds for retries to exhaust

# Query DLQ
docker exec -it $(docker-compose ps -q scylla) cqlsh -e \
  "SELECT event_type, error_message, failure_count, last_failed_at
   FROM orders_ks.dead_letter_queue;"
```

### Step 3: Prometheus Metrics

Open `src/metrics/mod.rs` to see available metrics:

```rust
pub struct Metrics {
    // CDC Processing
    pub cdc_events_processed: IntCounterVec,      // Total processed
    pub cdc_events_failed: IntCounterVec,         // Total failed
    pub cdc_processing_duration: HistogramVec,    // Latency distribution

    // Retry Metrics
    pub retry_attempts_total: IntCounterVec,      // Retry attempts
    pub retry_success: IntCounterVec,             // Successful retries
    pub retry_failure: IntCounterVec,             // Failed retries

    // DLQ Metrics
    pub dlq_messages_total: IntCounter,           // Total DLQ messages
    pub dlq_messages_by_event_type: IntCounterVec,// By event type

    // Circuit Breaker
    pub circuit_breaker_state: IntGauge,          // Current state
    pub circuit_breaker_transitions: IntCounterVec,// State transitions
}
```

**Exercise 7**: Query metrics and understand them:

```bash
# View all metrics
curl http://localhost:9090/metrics | less

# View specific metrics
curl -s http://localhost:9090/metrics | grep cdc_events_processed_total
curl -s http://localhost:9090/metrics | grep retry_attempts_total
curl -s http://localhost:9090/metrics | grep dlq_messages_total
```

**Understanding Histogram Metrics:**
```
cdc_processing_duration_seconds_bucket{event_type="OrderCreated",le="0.001"} 10
cdc_processing_duration_seconds_bucket{event_type="OrderCreated",le="0.005"} 42
cdc_processing_duration_seconds_bucket{event_type="OrderCreated",le="0.01"} 50
```

This means:
- 10 events processed in < 1ms
- 42 events processed in < 5ms
- 50 events processed in < 10ms

**Exercise 8**: Use the Makefile helper:

```bash
make metrics
```

---

## Testing Your Implementation

### Unit Tests

```bash
# Run all unit tests
cargo test

# Run specific test
cargo test test_retry_with_backoff

# Run with output
cargo test -- --nocapture
```

### Integration Tests

```bash
# Full end-to-end test
make integration-test

# This will:
# 1. Start services
# 2. Initialize schema
# 3. Build and run app
# 4. Verify metrics
# 5. Check DLQ
# 6. Validate Redpanda topics
```

### Manual Testing Scenarios

**Scenario 1: Happy Path**
```bash
make dev
# All events should publish successfully
# DLQ should be empty
```

**Scenario 2: Redpanda Failure**
```bash
make dev
# In another terminal:
docker-compose stop redpanda
# Wait for retries to exhaust
# Check DLQ for failed messages
```

**Scenario 3: Recovery**
```bash
# Continue from Scenario 2
docker-compose start redpanda
# Restart app
cargo run
# New events should publish successfully
```

**Scenario 4: High Load** (Advanced)
```bash
# Modify main.rs to create 1000 orders in a loop
# Observe metrics and CDC streaming performance
```

---

## Common Pitfalls

### Pitfall 1: Forgetting CDC is Asynchronous

**Wrong Expectation:**
```rust
create_order().await?;
// Event is NOT immediately in Redpanda!
```

**Reality:**
- Order is saved to ScyllaDB ✅
- Outbox message is saved ✅
- CDC captures change (< 100ms)
- Event published to Redpanda ✅

**Lesson**: Plan for eventual consistency in downstream services.

### Pitfall 2: Ignoring DLQ Messages

**Problem**: DLQ messages accumulate without monitoring.

**Solution**: Set up alerts on `dlq_messages_total`:
```yaml
# Prometheus alert rule
- alert: DLQMessagesAccumulating
  expr: dlq_messages_total > 10
  annotations:
    summary: "Dead letter queue has {{ $value }} messages"
```

### Pitfall 3: Not Testing Failure Scenarios

**Common mistake**: Only testing happy path.

**Best practice**: Test these scenarios:
- Redpanda down during order creation
- Network timeouts
- ScyllaDB slow queries
- Circuit breaker open state
- Multiple failures in sequence

### Pitfall 4: Incorrect Retry Configuration

**Too aggressive:**
```rust
RetryConfig {
    max_attempts: 100,        // Too many
    initial_delay: Duration::from_millis(1), // Too fast
    multiplier: 1.5,          // Too slow growth
}
```

**Result**: Overwhelms downstream, no time for recovery.

**Better:**
```rust
RetryConfig::aggressive()  // 5 attempts, 100ms → 500ms
```

---

## Next Steps

Congratulations! You now understand:
- ✅ The dual-write problem and outbox solution
- ✅ Transactional batched writes
- ✅ CDC streaming vs polling
- ✅ Retry mechanisms and exponential backoff
- ✅ Dead Letter Queue pattern
- ✅ Prometheus metrics and observability

### Going Deeper

1. **Read the Comparisons**: `COMPARISON.md` has detailed performance analysis
2. **Explore Phase Docs**: Read `PHASE3_CHANGES.md`, `PHASE4_CHANGES.md`, `PHASE5_CHANGES.md`
3. **Modify the Code**: Add new event types, change retry logic, add metrics
4. **Scale It**: Increase ScyllaDB replication factor, add Redpanda brokers

### Production Checklist

Before deploying to production:

- [ ] Configure appropriate retry timeouts for your use case
- [ ] Set up Prometheus monitoring and alerting
- [ ] Create operational runbooks for DLQ message handling
- [ ] Enable TLS for ScyllaDB and Redpanda
- [ ] Implement DLQ message replay mechanism
- [ ] Load test with expected traffic patterns
- [ ] Set up log aggregation (ELK, Splunk, etc.)
- [ ] Document disaster recovery procedures

### Additional Resources

- 📖 [ScyllaDB CDC Documentation](https://docs.scylladb.com/stable/using-scylla/cdc/)
- 📖 [Transactional Outbox Pattern](https://microservices.io/patterns/data/transactional-outbox.html)
- 📖 [Circuit Breaker Pattern](https://martinfowler.com/bliki/CircuitBreaker.html)
- 📖 [Prometheus Best Practices](https://prometheus.io/docs/practices/)

---

**Need Help?** Open an issue or check the FAQ in `docs/FAQ.md`

Happy Learning! 🎓

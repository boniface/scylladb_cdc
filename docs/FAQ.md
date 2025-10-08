# Frequently Asked Questions (FAQ)

## General Questions

### Q: What is the Transactional Outbox Pattern?

**A:** The Transactional Outbox Pattern solves the **dual-write problem** where you need to update a database AND publish a message atomically. Instead of two separate writes:

```rust
// ❌ Problem: Two separate writes
db.save(order).await?;           // What if this succeeds...
message_queue.publish(event).await?;  // ...but this fails?
```

You write BOTH to the database in a single transaction:

```rust
// ✅ Solution: Single transaction
db.batch([
    "INSERT INTO orders ...",
    "INSERT INTO outbox_messages ...",
]).await?;  // Both succeed or both fail
```

A separate process (CDC processor) reads the outbox and publishes events. This guarantees consistency.

---

### Q: Why use ScyllaDB instead of PostgreSQL?

**A:** This project uses ScyllaDB to demonstrate:
1. **High-performance CDC**: ScyllaDB's CDC is optimized for throughput
2. **Horizontal scalability**: Easy to add nodes
3. **Low latency**: < 100ms from write to CDC capture
4. **Rust ecosystem**: Great Rust driver support

PostgreSQL with Debezium is also a valid choice! The pattern is database-agnostic.

---

### Q: What's the difference between polling and CDC streaming?

**A:**

| Feature | Polling (Phase 2) | CDC Streaming (Phase 3) |
|---------|-------------------|-------------------------|
| **How it works** | Query `SELECT * FROM outbox WHERE created_at > ?` every N seconds | Database pushes changes via CDC streams |
| **Latency** | 0-N seconds (e.g., 0-5s) | < 100ms |
| **Database load** | Queries every interval, even if empty | No queries (push model) |
| **Scalability** | Limited by query performance | Excellent (parallel streams) |
| **Complexity** | Simple | Moderate |
| **When to use** | Educational purposes, simple cases | Production systems |

**This project implements BOTH** for educational comparison! See `COMPARISON.md` for details.

---

### Q: How does retry with exponential backoff work?

**A:** When a publish fails, we retry with increasing delays:

```
Attempt 1: Immediate
Attempt 2: After 100ms (if failed)
Attempt 3: After 200ms (100ms × 2)
Attempt 4: After 400ms (200ms × 2)
Attempt 5: After 500ms (400ms × 2, capped at 500ms)
```

If all retries fail, the message goes to the Dead Letter Queue (DLQ).

**Why exponential?**
- Gives the system time to recover
- Avoids overwhelming a struggling service
- Standard best practice for distributed systems

---

### Q: What happens if the CDC consumer crashes?

**A:** The system is resilient to crashes:

1. **Actix supervisor** detects the crash
2. **Restarts the consumer** automatically
3. **CDC library checkpoints** last processed position
4. **Resumes from checkpoint** (no messages lost)
5. **Idempotency** handles any duplicates

Example:
```
T0: Processing event #100
T1: Crash! ☠️
T2: Supervisor restarts consumer ♻️
T3: Resume from checkpoint (event #99)
T4: Re-process events #99-#100 (idempotent)
```

---

### Q: Why does the project have two CDC processors?

**A:** For **educational purposes**:

1. **`cdc_processor_polling.rs`** (Phase 2): Shows the simple polling approach
   - Easy to understand
   - Works with any database
   - Higher latency

2. **`cdc_stream_processor.rs`** (Phase 3): Shows the production approach
   - Real-time streaming
   - Lower latency
   - Better performance

You can compare both implementations side-by-side!

---

## Technical Questions

### Q: How do I ensure idempotency?

**A:** The project uses **event IDs** for idempotency:

```rust
// Each event has a unique ID
INSERT INTO outbox_messages (id, ...) VALUES (?, ...)

// Consumer tracks processed IDs
if processed_ids.contains(&event.id) {
    return; // Already processed, skip
}

// Or use message queue features
redpanda.publish(
    &topic,
    &event_id,  // ← Key used for deduplication
    &payload
)
```

**Best practices:**
1. Use UUIDs for event IDs
2. Store processed IDs with TTL (e.g., 24 hours)
3. Leverage message queue deduplication features
4. Design consumers to be idempotent (safe to run multiple times)

---

### Q: What should I do with Dead Letter Queue (DLQ) messages?

**A:** DLQ messages require manual intervention:

**Step 1: Investigate**
```sql
SELECT event_type, error_message, failure_count, last_failed_at
FROM dead_letter_queue
ORDER BY last_failed_at DESC
LIMIT 10;
```

**Step 2: Categorize**
- **Transient failures**: Redpanda was down → Safe to replay
- **Permanent failures**: Invalid data → Fix data first
- **Bug**: Code issue → Fix bug, then replay

**Step 3: Fix Root Cause**
- Redpanda down? → Ensure it's back up
- Circuit breaker open? → Wait for recovery
- Data issue? → Fix the data

**Step 4: Replay**
```rust
// Option 1: Manual replay via admin tool
for msg in dlq_messages {
    redpanda.publish(&msg.event_type, &msg.payload).await?;
    session.query("DELETE FROM dead_letter_queue WHERE id = ?", (msg.id,)).await?;
}

// Option 2: Automated replay job (cron)
// Retry DLQ messages older than 1 hour
```

**Step 5: Alert**
Set up alerts for DLQ accumulation:
```yaml
alert: DLQMessagesAccumulating
expr: dlq_messages_total > 10
```

---

### Q: How do I configure retry behavior?

**A:** Modify the `RetryConfig`:

```rust
// Aggressive (current default for CDC)
RetryConfig {
    max_attempts: 5,
    initial_delay: Duration::from_millis(100),
    max_delay: Duration::from_millis(500),
    multiplier: 2.0,
}

// Conservative (for expensive operations)
RetryConfig {
    max_attempts: 3,
    initial_delay: Duration::from_secs(1),
    max_delay: Duration::from_secs(10),
    multiplier: 3.0,
}

// Custom
RetryConfig {
    max_attempts: 10,           // More attempts
    initial_delay: Duration::from_millis(50),  // Faster start
    max_delay: Duration::from_secs(5),   // Higher cap
    multiplier: 1.5,            // Slower growth
}
```

**Guidelines:**
- **Transient errors** (network): Aggressive retries
- **Rate limits**: Conservative with longer delays
- **Expensive operations**: Fewer attempts, longer delays
- **Critical data**: More attempts, shorter delays

---

### Q: What metrics should I monitor?

**A:** Key metrics for production:

**1. CDC Processing**
```
cdc_events_processed_total{event_type="OrderCreated"}
cdc_processing_duration_seconds (histogram)
```
- **Alert if**: Processing stops for > 5 minutes
- **Goal**: < 100ms p95 latency

**2. Retry Metrics**
```
retry_attempts_total{operation="redpanda_publish"}
retry_failure_total{operation="redpanda_publish"}
```
- **Alert if**: Retry failures > 10% of attempts
- **Investigate**: Why are retries failing?

**3. DLQ Metrics**
```
dlq_messages_total
dlq_messages_by_event_type{event_type="OrderCreated"}
```
- **Alert if**: DLQ size > 100 messages
- **Action**: Investigate and replay

**4. Circuit Breaker**
```
circuit_breaker_state (0=Closed, 1=Open, 2=HalfOpen)
circuit_breaker_transitions_total{from_state="Closed",to_state="Open"}
```
- **Alert if**: Circuit opens frequently (> 5 times/hour)
- **Indicates**: Redpanda health issues

**5. Actor Health**
```
actor_health_status (0=Unhealthy, 1=Degraded, 2=Healthy)
```
- **Alert if**: Health != 2 for > 5 minutes

**Sample Dashboard:**
```
[CDC Throughput]  [CDC Latency (p95)]  [DLQ Size]
     ▲                    ▲                 ▲
   1000/s              50ms                 0

[Retry Success Rate]  [Circuit State]  [Actor Health]
         ▲                   ▲                ▲
       98%               Closed          Healthy
```

---

### Q: How does the circuit breaker work?

**A:** The circuit breaker has three states:

**CLOSED** (Normal)
- All calls allowed
- Tracks failures
- Opens after 5 consecutive failures

**OPEN** (Blocking)
- All calls immediately rejected
- Prevents cascading failures
- Waits 30 seconds before testing recovery

**HALF_OPEN** (Testing)
- Allows limited test calls
- If 3 succeed → back to CLOSED
- If any fail → back to OPEN

**Example:**
```
T0:   CLOSED - 5 failures
T1:   → OPEN (circuit trips)
T2-30: OPEN - all calls rejected
T31:  → HALF_OPEN (test recovery)
T32:  Test call 1: ✅ success
T33:  Test call 2: ✅ success
T34:  Test call 3: ✅ success
T35:  → CLOSED (recovered!)
```

**When to use:**
- External service calls (Redpanda, APIs)
- Database connections
- Any operation that can fail

**Configuration:**
```rust
CircuitBreakerConfig {
    failure_threshold: 5,          // Open after N failures
    timeout: Duration::from_secs(30), // Wait before testing
    success_threshold: 3,          // Close after N successes
}
```

---

## Operational Questions

### Q: How do I run this in production?

**A:** Production checklist:

**1. Infrastructure**
```yaml
# ScyllaDB Cluster
- Replication factor: 3
- Nodes: 3+ (for redundancy)
- Backup schedule: Daily

# Redpanda Cluster
- Brokers: 3+ (for HA)
- Replication factor: 3
- Retention: 7 days (or more)

# Application
- Instances: 2+ (for HA)
- Load balancer: Yes
- Health checks: /health endpoint
```

**2. Configuration**
```rust
// Tune retry config for your SLAs
RetryConfig {
    max_attempts: 5,
    initial_delay: Duration::from_millis(100),
    max_delay: Duration::from_secs(1),
    multiplier: 2.0,
}

// Adjust circuit breaker
CircuitBreakerConfig {
    failure_threshold: 10,  // More tolerance
    timeout: Duration::from_secs(60), // Longer recovery time
    success_threshold: 5,   // More confidence
}
```

**3. Monitoring**
- Deploy Prometheus for metrics collection
- Set up Grafana dashboards
- Configure alerts (PagerDuty, Slack, email)
- Enable structured logging (JSON format)
- Centralize logs (ELK, Splunk, Datadog)

**4. Security**
```toml
# Enable TLS for ScyllaDB
[scylla]
tls_enabled = true
cert_path = "/path/to/cert.pem"
key_path = "/path/to/key.pem"

# Enable authentication for Redpanda
[redpanda]
sasl_mechanism = "SCRAM-SHA-256"
username = "..."
password = "..."
```

**5. Operational Runbooks**
- Document DLQ message replay procedure
- Create rollback plan
- Define on-call escalation
- Test disaster recovery

---

### Q: How do I scale this system?

**A:** Scaling strategies:

**Vertical Scaling (Single Instance)**
```
Current: 1 app instance, 1 Scylla node, 1 Redpanda broker
Load: 1,000 events/sec

Upgrade: 4 CPU, 8GB RAM
New load: 5,000 events/sec
```

**Horizontal Scaling (Multiple Instances)**
```
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│   App Instance 1 │  │   App Instance 2 │  │   App Instance 3 │
│   (CDC Streams   │  │   (CDC Streams   │  │   (CDC Streams   │
│    0, 3, 6, ...) │  │    1, 4, 7, ...) │  │    2, 5, 8, ...) │
└────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘
         │                     │                     │
         └─────────────────────┴─────────────────────┘
                              │
                              ▼
                    ┌─────────────────────┐
                    │   ScyllaDB Cluster  │
                    │   (3+ nodes)        │
                    └─────────────────────┘
                              │
                              ▼
                    ┌─────────────────────┐
                    │  Redpanda Cluster   │
                    │   (3+ brokers)      │
                    └─────────────────────┘

Benefits:
• CDC streams automatically distributed
• No coordination needed between instances
• Linear scalability
• Fault tolerance
```

**Database Scaling**
```bash
# Add ScyllaDB node
docker-compose scale scylla=3

# ScyllaDB automatically:
# - Redistributes data
# - Rebalances CDC streams
# - Updates topology
```

**Message Queue Scaling**
```bash
# Add Redpanda broker
docker-compose scale redpanda=3

# Redpanda automatically:
# - Rebalances partitions
# - Maintains replication
# - Distributes load
```

**Load Testing**
```rust
// Generate load
for i in 0..10_000 {
    order_actor.send(CreateOrder { ... }).await?;
}

// Monitor metrics
curl http://localhost:9090/metrics | grep cdc_events_processed_total
```

---

### Q: How do I test this system?

**A:** Testing strategies:

**1. Unit Tests**
```bash
cargo test
```

Tests individual components:
- Retry logic
- Circuit breaker state machine
- Event serialization
- DLQ message creation

**2. Integration Tests**
```bash
make integration-test
```

Tests full flow:
- Start services (ScyllaDB, Redpanda)
- Initialize schema
- Create orders
- Verify CDC processing
- Check metrics
- Validate DLQ

**3. Failure Testing**
```bash
# Test Redpanda failure
docker-compose stop redpanda
cargo run
# Observe: Retries → DLQ

# Test ScyllaDB failure
docker-compose stop scylla
cargo run
# Observe: Connection errors

# Test circuit breaker
# Overwhelm Redpanda with messages
# Observe: Circuit opens → fast fails
```

**4. Load Testing**
```bash
# Use k6, Gatling, or custom script
cargo run --release  # Ensure release mode

# Monitor:
# - CPU usage
# - Memory usage
# - Metrics (throughput, latency)
# - Error rates
```

**5. Chaos Engineering**
```bash
# Random failures
while true; do
    docker-compose restart scylla
    sleep 30
    docker-compose restart redpanda
    sleep 30
done

# Observe system recovery
```

---

## Troubleshooting

### Q: Events are not appearing in Redpanda. What should I check?

**A:** Debug checklist:

1. **Verify events in outbox**
```sql
SELECT COUNT(*) FROM orders_ks.outbox_messages;
```
If 0 → Order actor not writing correctly

2. **Check CDC processor logs**
```bash
docker-compose logs app | grep CDC
```
Look for: "Starting CDC streaming" or errors

3. **Verify Redpanda is running**
```bash
docker-compose ps redpanda
```
Should show "Up"

4. **Check circuit breaker state**
```bash
curl -s http://localhost:9090/metrics | grep circuit_breaker_state
```
If 1 (OPEN) → Redpanda was down, circuit protecting

5. **Check DLQ**
```sql
SELECT * FROM orders_ks.dead_letter_queue;
```
Messages here indicate publish failures

6. **Verify CDC is enabled**
```sql
DESCRIBE TABLE orders_ks.outbox_messages;
```
Should show: `cdc = {'enabled': true}`

---

### Q: The application won't start. What's wrong?

**A:** Common issues:

**1. Port conflicts**
```bash
# Check if ports are in use
lsof -i :9042  # ScyllaDB
lsof -i :9092  # Redpanda
lsof -i :9090  # Metrics server

# Kill conflicting processes or change ports
```

**2. Docker not running**
```bash
docker ps
# If error, start Docker daemon
```

**3. Insufficient Docker resources**
```bash
# Check Docker settings
# Needs: 4GB+ RAM, 2+ CPUs

# Increase in Docker Desktop:
# Settings → Resources → Increase memory/CPU
```

**4. Database not ready**
```bash
# Wait for ScyllaDB to fully start
docker-compose logs scylla | grep "Starting listening for CQL clients"

# Usually takes 30-60 seconds on first start
```

**5. Compilation errors**
```bash
# Update Rust
rustup update

# Clean and rebuild
cargo clean
cargo build
```

---

### Q: My metrics show high latency. How do I optimize?

**A:** Optimization strategies:

**1. Identify bottleneck**
```bash
curl -s http://localhost:9090/metrics | grep duration

# Check p95/p99 latencies
cdc_processing_duration_seconds{quantile="0.95"}
```

**2. Database optimization**
```sql
-- Add indexes if querying
CREATE INDEX ON orders (customer_id);

-- Tune compaction
ALTER TABLE orders WITH compaction = {'class': 'LeveledCompactionStrategy'};

-- Increase batch size (if applicable)
```

**3. Application optimization**
```rust
// Use connection pooling
let session = SessionBuilder::new()
    .known_node("127.0.0.1:9042")
    .pool_size(PoolSize::PerShard(8))  // More connections
    .build()
    .await?;

// Batch multiple events
let mut batch = Batch::new();
for event in events {
    batch.append_statement("INSERT...");
}
session.batch(&batch, ...).await?;
```

**4. Network optimization**
```bash
# Use local Redpanda (not remote)
# Enable compression
# Use async/await efficiently
```

**5. Monitoring**
```bash
# Profile the application
cargo flamegraph --bin scylladb_cdc

# Analyze flamegraph.svg
```

---

For more questions, open an issue on GitHub or check the documentation:
- `README.md` - Project overview
- `TUTORIAL.md` - Step-by-step tutorial
- `CODE_WALKTHROUGH.md` - Detailed code explanations
- `DIAGRAMS.md` - Visual architecture diagrams

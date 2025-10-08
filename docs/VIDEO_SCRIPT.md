# Video Walkthrough Script: ScyllaDB CDC Outbox Pattern

This script can be used to create a video tutorial or live demo of the project.

**Duration**: 30-45 minutes
**Target Audience**: Software engineers learning about CDC and event-driven architecture
**Prerequisites**: Basic Rust, databases, and message queues knowledge

---

## Part 1: Introduction (5 minutes)

### Opening (30 seconds)

> "Hi! Today we're going to explore the Transactional Outbox Pattern using ScyllaDB CDC and Redpanda. By the end of this walkthrough, you'll understand how to reliably publish events from a database to a message queue without the dual-write problem."

**Screen**: Show project README, architecture diagram

---

### The Problem (2 minutes)

> "Let's start with the problem we're solving. Imagine you're building an e-commerce system. When a customer creates an order, you need to do two things:
>
> 1. Save the order to your database for persistence
> 2. Publish an event to a message queue so other services can react
>
> The naive approach looks like this:"

**Screen**: Show code editor with dual-write code

```rust
// ❌ Naive approach
async fn create_order(order: Order) -> Result<()> {
    database.save(&order).await?;        // Write 1
    message_queue.publish(event).await?; // Write 2
    Ok(())
}
```

> "This has a critical flaw: What if the database write succeeds but the message queue publish fails? You end up with an order in the database but no event published. Other services never find out about the order!"

**Screen**: Show diagram of failure scenario

> "This is called the dual-write problem, and it's one of the most common pitfalls in distributed systems."

---

### The Solution (2 minutes)

> "The Transactional Outbox Pattern solves this by writing BOTH the order and the event in a single database transaction:"

**Screen**: Show outbox pattern code

```rust
// ✅ Outbox pattern
async fn create_order(order: Order) -> Result<()> {
    let mut batch = Batch::new();
    batch.append("INSERT INTO orders ...");
    batch.append("INSERT INTO outbox_messages ...");
    database.batch(batch).await?;
    Ok(())
}
```

> "Now both writes succeed or both fail atomically. We've eliminated the inconsistency!"

**Screen**: Show architecture diagram with outbox table

> "But we still need to get events to the message queue. That's where Change Data Capture comes in. CDC automatically captures changes to the outbox table and streams them to a consumer process, which publishes to Redpanda."

**Screen**: Highlight CDC flow in architecture

---

## Part 2: Project Overview (3 minutes)

### Technology Stack (1 minute)

> "This project uses:
> - **ScyllaDB**: High-performance NoSQL database with built-in CDC
> - **Redpanda**: Kafka-compatible message broker
> - **Rust with Actix**: Actor-based concurrency
> - **scylla-cdc library**: For consuming CDC streams"

**Screen**: Show docker-compose.yml and Cargo.toml

---

### Project Phases (2 minutes)

> "The project is organized into phases to show the evolution from basic to production-ready:
>
> **Phase 1 & 2**: Foundation with transactional writes and polling-based CDC
> **Phase 3**: Real CDC streaming using scylla-cdc library
> **Phase 4**: Actor supervision and circuit breaker pattern
> **Phase 5**: DLQ, retry mechanism, and Prometheus metrics
>
> We'll focus on Phase 3-5 since they show production patterns."

**Screen**: Navigate through src/ directory structure

---

## Part 3: Demo - Starting the System (5 minutes)

### Setup (2 minutes)

> "Let's start the system. First, we need ScyllaDB and Redpanda:"

**Screen**: Terminal

```bash
# Show docker-compose.yml briefly
cat docker-compose.yml

# Start services
docker-compose up -d

# Show services starting
docker-compose ps
```

> "ScyllaDB takes about 30 seconds to fully start. Let's check the logs:"

```bash
docker-compose logs -f scylla | grep "Starting listening"
```

> "There it is! ScyllaDB is ready."

---

### Schema Initialization (1 minute)

> "Now let's initialize the database schema:"

**Screen**: Show schema.cql file

```bash
# Show the schema
cat src/db/schema.cql

# Initialize
docker exec $(docker-compose ps -q scylla) cqlsh -f /path/to/schema.cql
```

> "Notice the `WITH cdc = {'enabled': true}` on the outbox_messages table. This enables CDC for that table."

---

### Run the Application (2 minutes)

> "Let's start the application with debug logging to see everything:"

**Screen**: Split terminal - one for app, one for logs

```bash
# Terminal 1: Run app
RUST_LOG=debug cargo run

# Terminal 2: Watch specific logs
docker-compose logs -f app | grep -E "(CDC|Publishing)"
```

> "Watch what happens: The app starts, creates test orders, and the CDC processor picks up the events almost immediately!"

**Screen**: Highlight key log messages

---

## Part 4: Code Deep Dive (15 minutes)

### Transactional Writes (3 minutes)

> "Let's look at how transactional writes work. Open `src/actors/order_actor.rs`:"

**Screen**: Code editor, order_actor.rs

> "Here's the `persist_with_outbox` function. It's the heart of the pattern:"

**Navigate through code**, explaining:
1. Create a Batch
2. Append both INSERT statements
3. Execute atomically
4. Log success

> "The key is this `.batch()` call. ScyllaDB guarantees both statements succeed or both fail. No partial writes possible!"

**Screen**: Show batch execution highlighted

---

### CDC Streaming (4 minutes)

> "Now let's see how CDC streaming works. Open `src/actors/cdc_stream_processor.rs`:"

**Screen**: Code editor, cdc_stream_processor.rs

> "We implement the `Consumer` trait from scylla-cdc. This trait has one method: `consume_cdc()`. It's called for every change captured by CDC."

**Walk through consume_cdc()**:
1. Receive CDCRow with change data
2. Extract event fields (id, aggregate_id, event_type, payload)
3. Publish to Redpanda with retry
4. Handle success or send to DLQ

> "The beauty of this approach is it's push-based, not pull. No polling, no wasted queries. The database tells US when there's a change."

---

### Retry Mechanism (4 minutes)

> "Let's look at the retry logic. Open `src/utils/retry.rs`:"

**Screen**: Code editor, retry.rs

> "This is a generic retry function with exponential backoff:"

**Walk through retry_with_backoff()**:
1. Loop through attempts
2. Execute operation
3. On success, return immediately
4. On transient error, wait with exponential backoff
5. On permanent error or exhaustion, return failure

> "The backoff calculation is key:"

**Screen**: Highlight delay calculation

```rust
current_delay = std::cmp::min(
    Duration::from_millis(
        (current_delay.as_millis() as f64 * config.multiplier) as u64
    ),
    config.max_delay
);
```

> "This doubles the delay each time, but caps it at max_delay. So we get: 100ms, 200ms, 400ms, 500ms, 500ms..."

---

### Dead Letter Queue (2 minutes)

> "When all retries fail, we need a safety net. Open `src/actors/dlq_actor.rs`:"

**Screen**: Code editor, dlq_actor.rs

> "The DLQ actor persists failed messages to a dedicated table:"

**Show AddToDlq handler**:
1. Log error with full context
2. INSERT into dead_letter_queue
3. Include error message, failure count, timestamps

> "This allows manual intervention. Someone on-call can investigate, fix the root cause, and replay the messages."

---

### Metrics (2 minutes)

> "Finally, observability. Open `src/metrics/mod.rs`:"

**Screen**: Code editor, metrics/mod.rs

> "We expose Prometheus metrics for everything:"

**Scroll through metrics**:
- CDC events processed/failed
- Retry attempts/outcomes
- DLQ message counts
- Circuit breaker state

> "In production, you'd graph these in Grafana and set up alerts."

---

## Part 5: Failure Scenarios (8 minutes)

### Scenario 1: Redpanda Temporary Outage (3 minutes)

> "Let's see what happens when Redpanda goes down temporarily. This simulates a network blip or a brief outage."

**Screen**: Split terminal

```bash
# Terminal 1: Stop Redpanda
docker-compose stop redpanda

# Terminal 2: Watch app logs
docker-compose logs -f app | grep -E "(Attempt|Failed|Success)"
```

**Screen**: Start app or trigger new order

> "Watch the retry mechanism in action:"

**Point out log lines**:
- Attempt 1: Connection refused
- Attempt 2: After 100ms, still down
- Attempt 3: After 200ms, still down
- ...

```bash
# Restart Redpanda mid-retry
docker-compose start redpanda
```

> "And there! The next retry succeeds. The event was eventually delivered, and the order is safe."

---

### Scenario 2: Extended Outage → DLQ (3 minutes)

> "Now let's see what happens with an extended outage. Keep Redpanda down:"

**Screen**: Stop Redpanda, trigger orders

```bash
# Redpanda still down
docker-compose stop redpanda

# Create order
cargo run
```

> "After 5 attempts (about 1.2 seconds), the retries are exhausted:"

**Screen**: Show "Adding message to Dead Letter Queue" log

> "Let's check the DLQ:"

```bash
docker exec $(docker-compose ps -q scylla) cqlsh -e \
  "SELECT event_type, error_message, failure_count, last_failed_at
   FROM orders_ks.dead_letter_queue;"
```

> "There's our message! It's safely stored with full context. An on-call engineer can investigate and replay it later."

---

### Scenario 3: Circuit Breaker Protection (2 minutes)

> "The circuit breaker protects against cascading failures. Let's see it in action:"

**Screen**: Metrics endpoint

```bash
# Check circuit breaker state (0 = Closed)
curl -s http://localhost:9090/metrics | grep circuit_breaker_state
```

> "With Redpanda down and multiple failures, the circuit breaker opens:"

**Screen**: Show metrics changing from 0 to 1

```bash
# Circuit breaker state changed to 1 (Open)
circuit_breaker_state 1
```

> "Now all publish attempts are immediately rejected without even trying. This prevents overwhelming Redpanda when it comes back online."

---

## Part 6: Monitoring and Metrics (5 minutes)

### Metrics Endpoint (2 minutes)

> "Let's explore the metrics. The application exposes a Prometheus endpoint on port 9090:"

**Screen**: Browser, navigate to http://localhost:9090/metrics

> "Here are all the metrics. Let's look at some key ones:"

**Search for and explain**:
```
cdc_events_processed_total{event_type="OrderCreated"} 42
```

> "We've successfully processed 42 OrderCreated events."

```
cdc_processing_duration_seconds_bucket{event_type="OrderCreated",le="0.005"} 40
```

> "40 out of 42 were processed in under 5 milliseconds. That's fast!"

```
retry_attempts_total{operation="redpanda_publish",attempt="1"} 45
retry_attempts_total{operation="redpanda_publish",attempt="2"} 3
```

> "Most events (45) succeeded on first attempt. Only 3 needed a second attempt."

```
dlq_messages_total 0
```

> "Zero messages in DLQ. Perfect!"

---

### Makefile Helpers (1 minute)

> "The project includes a Makefile for common operations:"

**Screen**: Terminal

```bash
# Show available commands
make help

# View metrics via helper
make metrics
```

> "This formats the output nicely and shows just the important metrics."

---

### Integration Tests (2 minutes)

> "You can run full integration tests:"

**Screen**: Terminal

```bash
make integration-test
```

> "This script:
> 1. Starts all services
> 2. Initializes schemas
> 3. Runs the application
> 4. Verifies events were processed
> 5. Checks metrics
> 6. Validates DLQ is empty
> 7. Confirms Redpanda topics exist
>
> It's a complete end-to-end test!"

**Screen**: Show test output scrolling

---

## Part 7: Production Considerations (4 minutes)

### Scaling (1 minute)

> "In production, you'd run multiple instances of the application. CDC streams are automatically distributed:"

**Screen**: Show DIAGRAMS.md scaling section

> "Each instance consumes a subset of CDC streams. ScyllaDB handles the distribution automatically. You can add instances for horizontal scaling with zero coordination needed."

---

### Configuration (1 minute)

> "Tune the retry configuration for your SLA:"

**Screen**: Show RetryConfig in code

> "For critical data, use aggressive retries. For expensive operations, use conservative retries."

---

### Monitoring (1 minute)

> "In production, set up:
> - Prometheus to scrape metrics
> - Grafana for dashboards
> - Alerts for DLQ growth, circuit breaker opens, high latency
> - Log aggregation (ELK, Splunk, Datadog)"

**Screen**: Show example alert rules in FAQ.md

---

### Security (1 minute)

> "Enable:
> - TLS for ScyllaDB connections
> - Authentication for Redpanda
> - Secure metrics endpoint
> - Network policies"

**Screen**: Show security configuration examples

---

## Part 8: Wrap-Up (3 minutes)

### Key Takeaways (2 minutes)

> "Let's recap what we've learned:
>
> 1. **The Outbox Pattern** solves the dual-write problem with atomic transactions
> 2. **CDC Streaming** provides near real-time event delivery without polling
> 3. **Retry with Exponential Backoff** handles transient failures gracefully
> 4. **Dead Letter Queue** captures permanent failures for manual handling
> 5. **Circuit Breaker** protects against cascading failures
> 6. **Metrics** provide full observability into system behavior
>
> Together, these patterns create a production-ready, reliable event publishing system."

---

### Resources (1 minute)

> "To learn more:
> - Read `TUTORIAL.md` for hands-on exercises
> - Check `CODE_WALKTHROUGH.md` for detailed code explanations
> - See `DIAGRAMS.md` for visual architecture
> - Browse `FAQ.md` for common questions
> - Review `COMPARISON.md` for polling vs streaming analysis
>
> The project is on GitHub. Fork it, experiment, break things, learn!"

**Screen**: Show GitHub repository, README

---

### Call to Action (30 seconds)

> "If you found this helpful, star the repository and share it with your team. If you have questions or improvements, open an issue or PR.
>
> Thanks for watching, and happy learning!"

**Screen**: GitHub star button, closing title

---

## Recording Tips

### Pre-Recording Checklist

- [ ] Clean terminal history
- [ ] Set terminal font size to 16-18pt
- [ ] Use a simple, readable theme
- [ ] Close unnecessary applications
- [ ] Silence notifications
- [ ] Test audio levels
- [ ] Have a glass of water nearby
- [ ] Review script once more

### During Recording

- **Pace**: Speak slowly and clearly
- **Pauses**: Leave 2-3 seconds after showing code
- **Cursor**: Use it to highlight key lines
- **Mistakes**: Don't worry! Edit them out later
- **Enthusiasm**: Show genuine excitement
- **Breathing**: Remember to breathe naturally

### Post-Recording Editing

- Add chapter markers at each part
- Insert text overlays for key concepts
- Speed up long command executions (2x)
- Add background music (subtle, non-distracting)
- Include timestamps in description
- Add links to documentation

### Video Description Template

```
ScyllaDB CDC Outbox Pattern: Production-Ready Event Publishing

Learn how to implement the Transactional Outbox Pattern using ScyllaDB CDC and Redpanda.
This walkthrough covers everything from basic concepts to production-ready patterns including:
• Transactional writes
• CDC streaming
• Retry with exponential backoff
• Dead Letter Queue
• Circuit breaker
• Prometheus metrics

🔗 Repository: [GitHub URL]
📚 Documentation: [Docs URL]

⏱️ Timestamps:
0:00 - Introduction
0:30 - The Dual-Write Problem
2:30 - The Outbox Solution
5:00 - Project Overview
8:00 - Demo: Starting the System
13:00 - Code Deep Dive: Transactional Writes
16:00 - Code Deep Dive: CDC Streaming
20:00 - Code Deep Dive: Retry Mechanism
24:00 - Code Deep Dive: Dead Letter Queue
26:00 - Failure Scenarios
34:00 - Monitoring and Metrics
39:00 - Production Considerations
43:00 - Wrap-Up & Resources

#ScyllaDB #CDC #EventDriven #DistributedSystems #Rust #Kafka #Redpanda
```

---

## Alternative: Live Demo Format

If presenting live (e.g., conference talk), adjust the script:

1. **Shorter** (20-25 minutes)
2. **Pre-run** services (don't wait for startup)
3. **Focus** on 1-2 failure scenarios max
4. **Interactive**: Ask audience questions
5. **Backup**: Have screenshots ready if demo fails

Good luck with your video/demo! 🎥

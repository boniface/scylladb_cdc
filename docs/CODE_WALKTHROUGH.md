# Code Walkthrough: Deep Dive into Implementation

This document provides detailed, line-by-line explanations of the key components.

## 📑 Table of Contents

1. [Order Actor: Transactional Writes](#order-actor-transactional-writes)
2. [CDC Stream Processor: Event Consumption](#cdc-stream-processor-event-consumption)
3. [Retry Mechanism: Exponential Backoff](#retry-mechanism-exponential-backoff)
4. [DLQ Actor: Failed Message Handling](#dlq-actor-failed-message-handling)
5. [Circuit Breaker: Failure Protection](#circuit-breaker-failure-protection)
6. [Coordinator: Actor Supervision](#coordinator-actor-supervision)
7. [Metrics: Observability](#metrics-observability)

---

## Order Actor: Transactional Writes

**File**: `src/actors/order_actor.rs`

### The Core Function: persist_with_outbox

```rust
async fn persist_with_outbox<E: DomainEvent + serde::Serialize>(
    &self,
    order_query: &str,
    order_values: impl SerializeRow,
    event: &E,
) -> anyhow::Result<()> {
```

**Line-by-line breakdown:**

1. **Generic Constraint `E: DomainEvent + serde::Serialize`**
   - `E` must implement `DomainEvent` trait (provides event_type, aggregate_id, timestamp)
   - `E` must be serializable to JSON (for payload storage)
   - This allows any event type (OrderCreated, OrderUpdated, OrderCancelled)

2. **Parameters:**
   - `order_query`: SQL for inserting/updating the order
   - `order_values`: Values to bind to the query (must be serializable as a row)
   - `event`: The domain event to store in outbox

```rust
    // Step 1: Prepare event data
    let event_id = Uuid::new_v4();
    let payload = serde_json::to_string(event)?;
    let timestamp = Utc::now();
```

**Why these steps:**
- `event_id`: Unique identifier for idempotency checking
- `payload`: JSON string of the event (allows downstream services to deserialize)
- `timestamp`: Used for ordering and debugging

```rust
    // Step 2: Create a batched statement
    let mut batch = Batch::default();
    batch.append_statement(order_query);
    batch.append_statement(
        "INSERT INTO outbox_messages (id, aggregate_id, event_type, payload, created_at)
         VALUES (?, ?, ?, ?, ?)"
    );
```

**Critical concept:**
- `Batch::default()` creates a **logged batch** (default in ScyllaDB)
- Logged batches provide atomicity: all statements succeed or all fail
- Both INSERT statements are added to the same batch

```rust
    // Step 3: Execute the batch with both sets of values
    self.session.batch(
        &batch,
        (
            order_values,                      // Tuple 1: Values for order INSERT
            (                                  // Tuple 2: Values for outbox INSERT
                event_id,
                event.aggregate_id(),
                event.event_type(),
                payload,
                timestamp
            )
        )
    ).await?;
```

**Key insight:**
- The tuple structure matches the batch statements
- First element binds to first statement (order insert)
- Second element binds to second statement (outbox insert)
- `.await?` ensures errors propagate (transaction rolls back on failure)

```rust
    tracing::info!(
        order_id = %event.aggregate_id(),
        event_type = %event.event_type(),
        "✅ Transactionally persisted order and outbox event"
    );

    Ok(())
}
```

**Logging best practice:**
- Structured logging with key fields
- Makes it easy to filter logs by order_id or event_type
- The `%` syntax pretty-prints the values

### Usage Example: CreateOrder Handler

```rust
impl Handler<CreateOrder> for OrderActor {
    type Result = ResponseFuture<Result<Uuid, String>>;

    fn handle(&mut self, msg: CreateOrder, _: &mut Self::Context) -> Self::Result {
        let session = self.session.clone();
        let order_id = Uuid::new_v4();

        Box::pin(async move {
            tracing::info!(
                order_id = %order_id,
                customer_id = %msg.customer_id,
                "Creating new order"
            );

            // Create the domain event
            let event = OrderCreatedEvent {
                order_id,
                customer_id: msg.customer_id,
                items: msg.items.clone(),
                timestamp: Utc::now(),
            };

            // Serialize items to JSON
            let items_json = serde_json::to_string(&msg.items)
                .map_err(|e| format!("Failed to serialize items: {}", e))?;

            // Use persist_with_outbox for atomic write
            session
                .persist_with_outbox(
                    "INSERT INTO orders (id, customer_id, items, status, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?)",
                    (
                        order_id,
                        msg.customer_id,
                        items_json,
                        "pending",
                        event.timestamp,
                        event.timestamp,
                    ),
                    &event,
                )
                .await
                .map_err(|e| format!("Failed to persist order: {}", e))?;

            Ok(order_id)
        })
    }
}
```

**Flow:**
1. Generate unique order ID
2. Create OrderCreatedEvent
3. Serialize order items to JSON
4. Call persist_with_outbox with both order data and event
5. Return order ID if successful

**Error handling:**
- Any error causes the entire transaction to fail
- Order is not saved if event serialization fails
- Consistent state guaranteed

---

## CDC Stream Processor: Event Consumption

**File**: `src/actors/cdc_stream_processor.rs`

### The Consumer Implementation

```rust
pub struct OutboxCDCConsumer {
    redpanda: Arc<RedpandaClient>,
    dlq_actor: Option<Addr<DlqActor>>,
    retry_config: RetryConfig,
}
```

**Why these fields:**
- `redpanda`: Shared reference to Kafka client (thread-safe with Arc)
- `dlq_actor`: Address to send failed messages (Option because it's optional)
- `retry_config`: Configuration for retry attempts

```rust
impl OutboxCDCConsumer {
    pub fn new(redpanda: Arc<RedpandaClient>, dlq_actor: Option<Addr<DlqActor>>) -> Self {
        Self {
            redpanda,
            dlq_actor,
            retry_config: RetryConfig::aggressive(), // 5 attempts, fast retries
        }
    }
}
```

### The consume_cdc Method

```rust
#[async_trait]
impl Consumer for OutboxCDCConsumer {
    async fn consume_cdc(&mut self, data: CDCRow<'_>) -> anyhow::Result<()> {
```

**Important:** The `Consumer` trait is from the `scylla-cdc` library. By implementing it, our code is called for each CDC change.

```rust
        tracing::debug!(
            stream_id = ?data.stream_id,
            operation = %data.operation,
            "Received CDC row"
        );
```

**CDC concepts:**
- `stream_id`: Identifies which VNode group this change belongs to
- `operation`: INSERT, UPDATE, DELETE, etc.
- Multiple streams can be processed in parallel

```rust
        // Only process inserts - ignore updates/deletes on outbox table
        match data.operation {
            OperationType::RowInsert | OperationType::PostImage => {
```

**Why filter by operation:**
- We only care about NEW events (inserts)
- Updates to outbox_messages are not meaningful (events are immutable)
- Deletes would indicate outbox cleanup (optional feature)

```rust
                // Extract columns from CDC row
                let id = data.get_value("id")
                    .as_ref()
                    .and_then(|v| v.as_uuid())
                    .ok_or_else(|| anyhow::anyhow!("Missing or invalid id"))?;

                let aggregate_id = data.get_value("aggregate_id")
                    .as_ref()
                    .and_then(|v| v.as_uuid())
                    .ok_or_else(|| anyhow::anyhow!("Missing or invalid aggregate_id"))?;

                let event_type = data.get_value("event_type")
                    .as_ref()
                    .and_then(|v| v.as_text())
                    .map(|s| s.to_string())
                    .ok_or_else(|| anyhow::anyhow!("Missing or invalid event_type"))?;

                let payload = data.get_value("payload")
                    .as_ref()
                    .and_then(|v| v.as_text())
                    .map(|s| s.to_string())
                    .ok_or_else(|| anyhow::anyhow!("Missing or invalid payload"))?;
```

**Pattern explanation:**
- `data.get_value("column_name")` returns Option<&CqlValue>
- `.as_ref()` converts &Option<T> to Option<&T>
- `.and_then(|v| v.as_uuid())` extracts UUID if the value is a UUID
- `.ok_or_else(|| error)` converts Option<T> to Result<T, E>

**Why this pattern:**
- CDC rows are generic; we need type-safe extraction
- Errors here indicate schema mismatch (serious problem)
- Better to fail fast than proceed with invalid data

```rust
                // Publish to Redpanda with retry
                let redpanda = self.redpanda.clone();
                let event_type_clone = event_type.clone();
                let event_id = id;
                let first_attempt_time = Utc::now();

                let result = retry_with_backoff(
                    self.retry_config.clone(),
                    |attempt| {
                        let redpanda = redpanda.clone();
                        let event_type = event_type_clone.clone();
                        let event_id_str = event_id.to_string();
                        let payload = payload.clone();

                        async move {
                            tracing::debug!(
                                attempt = attempt,
                                event_id = %event_id,
                                "Attempting to publish event"
                            );

                            redpanda.publish(&event_type, &event_id_str, &payload).await
                        }
                    }
                ).await;
```

**Retry mechanism:**
- Clone necessary data for the async closure
- Each attempt logs its number for debugging
- The closure is called multiple times until success or exhaustion

```rust
                match result {
                    RetryResult::Success(_) => {
                        tracing::info!(
                            event_id = %event_id,
                            event_type = %event_type,
                            "✅ Successfully published event via CDC stream"
                        );
                        Ok(())
                    }
                    RetryResult::Failed(e) | RetryResult::PermanentFailure(e) => {
                        tracing::error!(
                            error = %e,
                            event_id = %event_id,
                            event_type = %event_type,
                            "❌ Failed to publish event after retries, sending to DLQ"
                        );

                        // Send to Dead Letter Queue
                        if let Some(ref dlq) = self.dlq_actor {
                            dlq.do_send(AddToDlq {
                                id: event_id,
                                aggregate_id,
                                event_type: event_type.clone(),
                                payload,
                                error_message: e.to_string(),
                                failure_count: self.retry_config.max_attempts as i32,
                                first_failed_at: first_attempt_time,
                            });
                        }

                        // Don't propagate error - message is in DLQ
                        Ok(())
                    }
                }
```

**Critical decision:**
- On failure, send to DLQ but return Ok(())
- Why? Because we've handled the failure (stored in DLQ)
- Returning Err would crash the CDC consumer
- DLQ allows manual intervention later

### Starting the CDC Log Reader

```rust
pub async fn start_cdc_streaming(&self) -> anyhow::Result<()> {
    tracing::info!("🔄 Starting CDC streaming for outbox_messages table");

    let factory = Arc::new(OutboxConsumerFactory::new(
        self.redpanda.clone(),
        self.dlq_actor.clone()
    ));

    // Build the CDC log reader
    let (_reader, handle) = CDCLogReaderBuilder::new()
        .session(self.session.clone())
        .keyspace(KEYSPACE)
        .table_name(TABLE)
        .consumer_factory(factory)
        .build()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create CDC log reader: {}", e))?;
```

**What happens here:**
1. Create a consumer factory (creates consumer instances per stream)
2. Build the CDC log reader with:
   - Database session
   - Keyspace and table to monitor
   - Factory for creating consumers
3. The reader automatically discovers CDC streams and topology

```rust
    tracing::info!("✅ CDC log reader started successfully");
    tracing::info!("🎯 Listening for changes to {}.{}", KEYSPACE, TABLE);

    // Spawn the handle to run in the background
    tokio::spawn(async move {
        match handle.await {
            Ok(_) => {
                tracing::info!("CDC reader completed successfully");
            }
            Err(e) => {
                tracing::error!(error = %e, "CDC reader failed");
            }
        }
    });

    Ok(())
}
```

**Background execution:**
- `tokio::spawn` runs the CDC reader in a separate task
- The reader runs forever, consuming CDC changes
- Errors are logged but don't crash the main application
- The function returns immediately, allowing other work

---

## Retry Mechanism: Exponential Backoff

**File**: `src/utils/retry.rs`

### The Core Retry Function

```rust
pub async fn retry_with_backoff<F, Fut, T, E>(
    config: RetryConfig,
    mut operation: F,
) -> RetryResult<T, E>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display + IsTransient,
```

**Generic constraints explained:**
- `F: FnMut(u32) -> Fut`: Function that takes attempt number, returns a Future
- `Fut: Future<Output = Result<T, E>>`: The Future returns a Result
- `E: Display + IsTransient`: Error must be printable and classify as transient/permanent

```rust
{
    let mut current_delay = config.initial_delay;

    for attempt in 1..=config.max_attempts {
        tracing::debug!(
            attempt = attempt,
            max_attempts = config.max_attempts,
            delay_ms = current_delay.as_millis(),
            "Retry attempt"
        );
```

**Loop structure:**
- Iterate from 1 to max_attempts (inclusive)
- Track current_delay for exponential backoff
- Log each attempt with context

```rust
        // Execute the operation
        match operation(attempt).await {
            Ok(result) => {
                if attempt > 1 {
                    tracing::info!(
                        attempt = attempt,
                        "Operation succeeded after retry"
                    );
                }
                return RetryResult::Success(result);
            }
```

**Success case:**
- If operation succeeds, return immediately with result
- Log success if it wasn't the first attempt (indicates recovery)
- No more retries needed

```rust
            Err(e) => {
                // Check if error is transient or permanent
                if !e.is_transient() {
                    tracing::error!(
                        error = %e,
                        "Permanent error, not retrying"
                    );
                    return RetryResult::PermanentFailure(e);
                }
```

**Permanent failure check:**
- Some errors should not be retried (e.g., invalid input, auth failure)
- `is_transient()` trait method classifies the error
- Return immediately to avoid wasting retry attempts

```rust
                // Last attempt?
                if attempt == config.max_attempts {
                    tracing::error!(
                        error = %e,
                        attempt = attempt,
                        "All retry attempts exhausted"
                    );
                    return RetryResult::Failed(e);
                }
```

**Exhaustion check:**
- If this was the last attempt, return failure
- Don't sleep if there are no more retries

```rust
                // Log transient error and wait
                tracing::warn!(
                    error = %e,
                    attempt = attempt,
                    delay_ms = current_delay.as_millis(),
                    "Transient error, retrying after delay"
                );

                tokio::time::sleep(current_delay).await;
```

**Backoff delay:**
- Log the error as a warning (not an error, since we'll retry)
- Sleep for current_delay duration
- This gives the system time to recover

```rust
                // Calculate next delay with exponential backoff
                current_delay = std::cmp::min(
                    Duration::from_millis(
                        (current_delay.as_millis() as f64 * config.multiplier) as u64
                    ),
                    config.max_delay
                );
            }
        }
    }
```

**Exponential backoff calculation:**
- Multiply current delay by multiplier (e.g., 2.0 for doubling)
- Cap at max_delay to prevent excessive waiting
- `std::cmp::min` ensures we never exceed max_delay

**Example:**
```
Initial: 100ms
After attempt 1: min(100ms * 2.0, 500ms) = 200ms
After attempt 2: min(200ms * 2.0, 500ms) = 400ms
After attempt 3: min(400ms * 2.0, 500ms) = 500ms (capped)
After attempt 4: min(500ms * 2.0, 500ms) = 500ms (capped)
```

```rust
    // Should never reach here due to return statements above
    unreachable!("Retry logic should have returned by now")
}
```

**Safety:**
- Every code path returns a RetryResult
- This line is unreachable but satisfies the compiler
- If it's ever reached, it indicates a logic bug

### The IsTransient Trait

```rust
pub trait IsTransient {
    fn is_transient(&self) -> bool;
}
```

**Purpose:**
- Allows error types to classify themselves
- Transient: Network timeout, connection refused, rate limit
- Permanent: Invalid auth, malformed request, resource not found

**Example implementations:**

```rust
impl IsTransient for std::io::Error {
    fn is_transient(&self) -> bool {
        use std::io::ErrorKind;
        matches!(
            self.kind(),
            ErrorKind::ConnectionRefused
                | ErrorKind::ConnectionReset
                | ErrorKind::TimedOut
                | ErrorKind::WouldBlock
        )
    }
}

impl IsTransient for anyhow::Error {
    fn is_transient(&self) -> bool {
        // Conservative: assume all errors are transient
        // In production, you'd check error messages/types
        true
    }
}
```

---

## DLQ Actor: Failed Message Handling

**File**: `src/actors/dlq_actor.rs`

### The AddToDlq Handler

```rust
impl Handler<AddToDlq> for DlqActor {
    type Result = ResponseFuture<Result<(), String>>;

    fn handle(&mut self, msg: AddToDlq, _: &mut Self::Context) -> Self::Result {
        let session = self.session.clone();
        let now = Utc::now();
```

**Actor pattern:**
- Handler receives a message and context
- Returns ResponseFuture for async processing
- Clone session for use in async block

```rust
        tracing::error!(
            event_id = %msg.id,
            event_type = %msg.event_type,
            aggregate_id = %msg.aggregate_id,
            error = %msg.error_message,
            failure_count = msg.failure_count,
            "💀 Adding message to Dead Letter Queue"
        );
```

**Why log as error:**
- DLQ messages indicate operational issues
- Should trigger alerts in production
- Structured logging allows filtering by event_type

```rust
        Box::pin(async move {
            session
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
                        now,  // last_failed_at
                        now,  // created_at
                    ),
                )
                .await
                .map_err(|e| format!("Failed to insert into DLQ: {}", e))?;
```

**Data captured:**
- Original event data (id, aggregate_id, event_type, payload)
- Error context (error_message, failure_count)
- Timestamps (first_failed_at, last_failed_at, created_at)

**Why this matters:**
- Allows manual inspection and replay
- failure_count helps identify systematic issues
- Timestamps show when problem started

```rust
            tracing::info!(
                event_id = %msg.id,
                "Message successfully stored in DLQ"
            );

            Ok(())
        })
    }
}
```

---

## Circuit Breaker: Failure Protection

**File**: `src/utils/circuit_breaker.rs`

### The CircuitBreaker Struct

```rust
pub struct CircuitBreaker {
    state: Arc<Mutex<CircuitState>>,
    config: CircuitBreakerConfig,
    failure_count: Arc<AtomicU32>,
    success_count: Arc<AtomicU32>,
    last_failure_time: Arc<Mutex<Option<Instant>>>,
}
```

**Field breakdown:**
- `state`: Current state (Closed/Open/HalfOpen) - protected by Mutex for thread safety
- `config`: Configuration (thresholds, timeout)
- `failure_count`: Atomic counter for failures (lock-free)
- `success_count`: Atomic counter for successes in HalfOpen state
- `last_failure_time`: When the last failure occurred

**Why Arc and Mutex?**
- `Arc`: Shared ownership across threads
- `Mutex`: Ensures only one thread modifies state at a time
- `AtomicU32`: Lock-free counters for performance

### The call Method

```rust
pub async fn call<F, T, E>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>
where
    F: std::future::Future<Output = Result<T, E>>,
{
    // 1. Check current state
    let current_state = {
        let state = self.state.lock().unwrap();
        state.clone()
    };
```

**State check:**
- Lock the state mutex briefly
- Clone the state (cheap - it's an enum)
- Release lock immediately (minimize contention)

```rust
    match current_state {
        CircuitState::Open => {
            // Check if timeout has elapsed
            let last_failure = self.last_failure_time.lock().unwrap();
            if let Some(last_time) = *last_failure {
                if last_time.elapsed() >= self.config.timeout {
                    drop(last_failure); // Release lock before transitioning
                    self.transition_to_half_open();
                    // Continue to execute in HalfOpen state
                } else {
                    // Circuit is open, reject immediately
                    return Err(CircuitBreakerError::CircuitOpen);
                }
            }
        }
        CircuitState::Closed => {
            // Normal operation - proceed
        }
        CircuitState::HalfOpen => {
            // Testing recovery - proceed with caution
        }
    }
```

**State-based behavior:**
- **Open**: Check if timeout elapsed
  - If yes → transition to HalfOpen and continue
  - If no → reject immediately (fast fail)
- **Closed**: Normal operation, execute the operation
- **HalfOpen**: Testing phase, execute carefully

```rust
    // 2. Execute the operation
    match operation.await {
        Ok(result) => {
            self.on_success();
            Ok(result)
        }
        Err(e) => {
            self.on_failure();
            Err(CircuitBreakerError::OperationFailed(e))
        }
    }
}
```

**Execution and callbacks:**
- Execute the operation
- On success → call `on_success()` to update state
- On failure → call `on_failure()` to update state
- Return wrapped result

### State Transition Methods

```rust
fn on_success(&self) {
    let current_state = self.state.lock().unwrap().clone();

    match current_state {
        CircuitState::HalfOpen => {
            // Increment success counter
            let success_count = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;

            if success_count >= self.config.success_threshold {
                // Enough successes, close the circuit
                let mut state = self.state.lock().unwrap();
                *state = CircuitState::Closed;
                self.failure_count.store(0, Ordering::SeqCst);
                self.success_count.store(0, Ordering::SeqCst);

                tracing::info!("Circuit breaker closed after {} successes", success_count);
            }
        }
        CircuitState::Closed => {
            // Reset failure count on success
            self.failure_count.store(0, Ordering::SeqCst);
        }
        CircuitState::Open => {
            // Should not happen during Open state
        }
    }
}
```

**Success handling:**
- **HalfOpen**: Count successes
  - If threshold reached (3) → Close the circuit
  - Reset all counters
- **Closed**: Reset failure counter (system is healthy)

```rust
fn on_failure(&self) {
    let current_state = self.state.lock().unwrap().clone();

    // Update last failure time
    let mut last_failure = self.last_failure_time.lock().unwrap();
    *last_failure = Some(Instant::now());
    drop(last_failure);

    match current_state {
        CircuitState::Closed => {
            // Increment failure counter
            let failure_count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;

            if failure_count >= self.config.failure_threshold {
                // Too many failures, open the circuit
                let mut state = self.state.lock().unwrap();
                *state = CircuitState::Open;
                self.success_count.store(0, Ordering::SeqCst);

                tracing::warn!("Circuit breaker opened after {} failures", failure_count);
            }
        }
        CircuitState::HalfOpen => {
            // Any failure in HalfOpen immediately reopens
            let mut state = self.state.lock().unwrap();
            *state = CircuitState::Open;
            self.success_count.store(0, Ordering::SeqCst);

            tracing::warn!("Circuit breaker reopened due to failure in HalfOpen state");
        }
        CircuitState::Open => {
            // Already open, no action needed
        }
    }
}
```

**Failure handling:**
- Record timestamp of failure
- **Closed**: Increment counter
  - If threshold reached (5) → Open the circuit
- **HalfOpen**: Any failure → Reopen immediately
  - This prevents premature recovery

**Why this design?**
- Conservative recovery: One failure in HalfOpen reopens circuit
- Prevents cascading failures
- Gives systems time to fully recover

---

## Coordinator: Actor Supervision

**File**: `src/actors/coordinator.rs`

### The Coordinator Structure

```rust
pub struct CoordinatorActor {
    session: Arc<Session>,
    redpanda: Arc<RedpandaClient>,
    order_actor: Option<Addr<OrderActor>>,
    cdc_processor: Option<Addr<CdcStreamProcessor>>,
    health_check: Option<Addr<HealthCheckActor>>,
    dlq_actor: Option<Addr<DlqActor>>,
}
```

**Supervision tree:**
```
CoordinatorActor (root supervisor)
├── HealthCheckActor
├── DlqActor
├── OrderActor
└── CdcStreamProcessor
```

**Option wrappers:**
- Actors are `Option` because they're created after the coordinator
- `None` initially, `Some(addr)` after starting

### Starting Child Actors

```rust
fn start_child_actors(&mut self, ctx: &mut Context<Self>) {
    tracing::info!("Starting supervised child actors");

    // 1. Start health check actor (monitors everything)
    let health_check = HealthCheckActor::new(self.redpanda.clone()).start();
    self.health_check = Some(health_check.clone());

    // 2. Start DLQ actor (handles failures)
    let dlq_actor = DlqActor::new(self.session.clone()).start();
    self.dlq_actor = Some(dlq_actor.clone());

    // Report DLQ health
    health_check.do_send(UpdateHealth {
        component: "dlq_actor".to_string(),
        status: HealthStatus::Healthy,
        details: Some("DLQ actor started".to_string()),
    });
```

**Startup order:**
1. HealthCheck first (monitors all others)
2. DLQ next (needed by CDC processor)
3. Report health status

**Why clone addresses?**
- Actors communicate via addresses (`Addr<T>`)
- Addresses are cheaply cloneable (Arc internally)
- Each component needs its own copy

```rust
    // 3. Start order actor (handles commands)
    let order_actor = OrderActor::new(self.session.clone()).start();
    self.order_actor = Some(order_actor.clone());

    health_check.do_send(UpdateHealth {
        component: "order_actor".to_string(),
        status: HealthStatus::Healthy,
        details: Some("Order actor started".to_string()),
    });

    // 4. Start CDC stream processor (consumes events)
    let cdc_processor = CdcStreamProcessor::new(
        self.session.clone(),
        self.redpanda.clone(),
        Some(dlq_actor.clone()),  // Pass DLQ address
    ).start();
    self.cdc_processor = Some(cdc_processor.clone());

    health_check.do_send(UpdateHealth {
        component: "cdc_processor".to_string(),
        status: HealthStatus::Healthy,
        details: Some("CDC processor started".to_string()),
    });

    tracing::info!("✅ All supervised actors started successfully");
}
```

**Dependency injection:**
- CDC processor receives DLQ actor address
- This allows CDC to send failed messages to DLQ
- Decoupled design - actors communicate via messages

### Graceful Shutdown

```rust
impl Handler<Shutdown> for CoordinatorActor {
    type Result = Result<(), String>;

    fn handle(&mut self, _msg: Shutdown, ctx: &mut Self::Context) -> Self::Result {
        tracing::info!("Received shutdown signal");

        // Stop child actors in reverse order
        if let Some(ref order_actor) = self.order_actor {
            order_actor.do_send(StopActor);
        }

        if let Some(ref cdc_processor) = self.cdc_processor {
            cdc_processor.do_send(StopActor);
        }

        if let Some(ref dlq_actor) = self.dlq_actor {
            dlq_actor.do_send(StopActor);
        }

        if let Some(ref health_check) = self.health_check {
            health_check.do_send(StopActor);
        }

        // Stop coordinator itself
        ctx.stop();

        Ok(())
    }
}
```

**Shutdown sequence:**
1. Send `StopActor` to all children
2. Each child actor handles cleanup
3. Coordinator stops itself
4. System gracefully terminates

**Why `do_send` instead of `send`?**
- `do_send`: Fire-and-forget (async message)
- Don't wait for responses
- Faster shutdown

### Periodic Health Checks

```rust
impl Actor for CoordinatorActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("🎯 CoordinatorActor started");
        self.start_child_actors(ctx);

        // Schedule periodic health checks
        ctx.run_interval(
            std::time::Duration::from_secs(30),
            |act, _ctx| {
                if let Some(ref health_check) = act.health_check {
                    let health_check = health_check.clone();
                    actix::spawn(async move {
                        match health_check.send(GetSystemHealth).await {
                            Ok(health) => {
                                match health.overall_status {
                                    HealthStatus::Healthy => {
                                        tracing::debug!("System health: Healthy");
                                    }
                                    HealthStatus::Degraded(ref msg) => {
                                        tracing::warn!("System health: Degraded - {}", msg);
                                    }
                                    HealthStatus::Unhealthy(ref msg) => {
                                        tracing::error!("System health: Unhealthy - {}", msg);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("Failed to get system health: {}", e);
                            }
                        }
                    });
                }
            },
        );
    }
}
```

**Health check interval:**
- Every 30 seconds
- Query health check actor
- Log status based on severity
- Spawns async task to avoid blocking

---

## Metrics: Observability

**File**: `src/metrics/mod.rs`

### Metrics Registry

```rust
pub struct Metrics {
    registry: Registry,

    // CDC Processing Metrics
    pub cdc_events_processed: IntCounterVec,
    pub cdc_events_failed: IntCounterVec,
    pub cdc_processing_duration: HistogramVec,

    // Retry Metrics
    pub retry_attempts_total: IntCounterVec,
    pub retry_success: IntCounterVec,
    pub retry_failure: IntCounterVec,

    // DLQ Metrics
    pub dlq_messages_total: IntCounter,
    pub dlq_messages_by_event_type: IntCounterVec,

    // Circuit Breaker Metrics
    pub circuit_breaker_state: IntGauge,
    pub circuit_breaker_transitions: IntCounterVec,

    // Actor Metrics
    pub actor_health_status: IntGauge,
    pub messages_sent: IntCounterVec,
    pub messages_received: IntCounterVec,
}
```

**Metric types explained:**

1. **Counter** (`IntCounter`, `IntCounterVec`):
   - Only goes up
   - Examples: events processed, retry attempts
   - `Vec` variant has labels (e.g., by event_type)

2. **Gauge** (`IntGauge`):
   - Can go up or down
   - Examples: circuit breaker state, actor health
   - Current value at any point in time

3. **Histogram** (`HistogramVec`):
   - Distribution of values
   - Examples: processing duration
   - Automatically calculates percentiles (p50, p95, p99)

### Creating Metrics

```rust
impl Metrics {
    pub fn new() -> anyhow::Result<Self> {
        let registry = Registry::new();

        // Create a counter with labels
        let cdc_events_processed = IntCounterVec::new(
            Opts::new(
                "cdc_events_processed_total",           // Metric name
                "Total CDC events processed"            // Description
            ),
            &["event_type"],                            // Label names
        )?;
        registry.register(Box::new(cdc_events_processed.clone()))?;
```

**Registration steps:**
1. Create metric with name, description, labels
2. Register with Prometheus registry
3. Store reference in Metrics struct

**Why labels?**
- Allow filtering/grouping in queries
- Example: `cdc_events_processed_total{event_type="OrderCreated"}`

```rust
        // Create a histogram with custom buckets
        let cdc_processing_duration = HistogramVec::new(
            HistogramOpts::new(
                "cdc_processing_duration_seconds",
                "CDC event processing duration"
            ).buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]),
            &["event_type"],
        )?;
        registry.register(Box::new(cdc_processing_duration.clone()))?;
```

**Histogram buckets:**
- Define ranges for distribution
- Buckets: < 1ms, < 5ms, < 10ms, < 50ms, < 100ms, < 500ms, < 1s, < 5s
- Allows calculating percentiles

### Helper Methods

```rust
    /// Helper to record CDC event processing
    pub fn record_cdc_event(&self, event_type: &str, duration_secs: f64, success: bool) {
        if success {
            self.cdc_events_processed.with_label_values(&[event_type]).inc();
        } else {
            self.cdc_events_failed.with_label_values(&[event_type, "processing_error"]).inc();
        }
        self.cdc_processing_duration.with_label_values(&[event_type]).observe(duration_secs);
    }
```

**Helper benefits:**
- Single method updates multiple metrics
- Consistent labeling
- Easier to use from application code

**Usage:**
```rust
let start = Instant::now();
// Process event
let duration = start.elapsed().as_secs_f64();
metrics.record_cdc_event("OrderCreated", duration, true);
```

### Metrics Server

**File**: `src/metrics/server.rs`

```rust
pub async fn start_metrics_server(registry: Arc<Registry>, port: u16) -> std::io::Result<()> {
    tracing::info!("📊 Starting metrics server on http://0.0.0.0:{}/metrics", port);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(registry.clone()))
            .route("/metrics", web::get().to(metrics_handler))
            .route("/health", web::get().to(health_handler))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
```

**HTTP endpoints:**
- `/metrics`: Prometheus exposition format
- `/health`: Simple health check

```rust
async fn metrics_handler(registry: web::Data<Arc<Registry>>) -> impl Responder {
    let encoder = TextEncoder::new();
    let metric_families = registry.gather();

    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();

    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")  // Prometheus format
        .body(buffer)
}
```

**Metrics handler:**
1. Gather all metrics from registry
2. Encode to Prometheus text format
3. Return as HTTP response

**Output format:**
```
# HELP cdc_events_processed_total Total CDC events processed
# TYPE cdc_events_processed_total counter
cdc_events_processed_total{event_type="OrderCreated"} 42
cdc_events_processed_total{event_type="OrderUpdated"} 15
```

---

## Summary: Key Design Patterns

### 1. Transactional Outbox Pattern
- Solves dual-write problem
- Atomic batched writes
- Eventual consistency

### 2. CDC Streaming
- Push-based, not pull
- Near real-time (< 100ms)
- Automatic stream distribution

### 3. Retry with Exponential Backoff
- Handles transient failures
- Prevents overwhelming systems
- Configurable policies

### 4. Dead Letter Queue
- Safety net for permanent failures
- Preserves data for manual handling
- Full error context

### 5. Circuit Breaker
- Prevents cascading failures
- Fast-fail when system is down
- Automatic recovery testing

### 6. Actor Supervision
- Hierarchical fault isolation
- Graceful shutdown
- Coordinated lifecycle management

### 7. Comprehensive Metrics
- Observability into all operations
- Prometheus standard format
- Production-ready monitoring

---

This completes the code walkthrough! You now have a deep understanding of:
- **What** each component does
- **Why** design decisions were made
- **How** the code implements the patterns

Next steps:
- Read `DIAGRAMS.md` for visual understanding
- Try `TUTORIAL.md` for hands-on exercises
- Check `FAQ.md` for specific questions

Happy coding! 🚀

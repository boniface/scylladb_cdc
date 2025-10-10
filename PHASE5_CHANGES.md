# Phase 5: Production Readiness - DLQ, Retry, and Observability

## 🎯 Overview

Phase 5 completes the production readiness of our ScyllaDB CDC Outbox Pattern implementation by adding:

1. **Dead Letter Queue (DLQ)** - For failed message handling
2. **Retry Mechanism** - Exponential backoff retry strategy
3. **Prometheus Metrics** - Comprehensive observability
4. **Integration Tests** - Full end-to-end testing

This phase ensures the system is resilient, observable, and ready for production deployment.

---

## 🔧 What Was Implemented

### 1. Dead Letter Queue (DLQ)

**Purpose**: Capture and store messages that fail processing after all retry attempts, allowing for manual intervention and analysis.

**Files Created/Modified**:
- `src/db/dlq_schema.cql` - DLQ table schema
- `src/actors/dlq_actor.rs` - DLQ actor implementation

**Schema**:
```sql
CREATE TABLE dead_letter_queue (
    id UUID PRIMARY KEY,
    aggregate_id UUID,
    event_type TEXT,
    payload TEXT,
    error_message TEXT,
    failure_count INT,
    first_failed_at TIMESTAMP,
    last_failed_at TIMESTAMP,
    created_at TIMESTAMP
);
```

**Features**:
- Persistent storage of failed messages
- Indexed by event_type, aggregate_id, and timestamp
- Query support for retrieving DLQ messages
- Statistics tracking (total messages, breakdown by event type)

**Actor Messages**:
```rust
// Add a message to DLQ
AddToDlq {
    id, aggregate_id, event_type, payload,
    error_message, failure_count, first_failed_at
}

// Retrieve DLQ messages
GetDlqMessages { limit: i32 }

// Get DLQ statistics
GetDlqStats
```

---

### 2. Retry Mechanism with Exponential Backoff

**Purpose**: Automatically retry failed operations with increasing delays to handle transient failures gracefully.

**File Created**: `src/utils/retry.rs`

**Key Features**:
- Configurable retry attempts, delays, and multipliers
- Exponential backoff with max delay cap
- Predefined configurations (aggressive, conservative)
- Detailed logging of retry attempts

**Configuration**:
```rust
pub struct RetryConfig {
    pub max_attempts: u32,      // Maximum retry attempts
    pub initial_delay: Duration, // First retry delay
    pub max_delay: Duration,     // Maximum delay cap
    pub multiplier: f64,         // Exponential multiplier
}
```

**Presets**:
```rust
// Aggressive: 5 attempts, 100ms → 500ms
RetryConfig::aggressive()

// Conservative: 3 attempts, 1s → 10s
RetryConfig::conservative()
```

**Usage in CDC Processor**:
```rust
let result = retry_with_backoff(
    RetryConfig::aggressive(),
    |attempt| {
        redpanda.publish(&event_type, &event_id, &payload)
    }
).await;

match result {
    RetryResult::Success(_) => { /* Success */ }
    RetryResult::Failed(_) | RetryResult::PermanentFailure(_) => {
        // Send to DLQ
        dlq_actor.do_send(AddToDlq { ... });
    }
}
```

**Retry Flow**:
```
Attempt 1: Immediate
   ↓ (failure)
Wait 100ms
   ↓
Attempt 2: After 100ms
   ↓ (failure)
Wait 200ms (100ms * 2)
   ↓
Attempt 3: After 200ms
   ↓ (failure)
Wait 400ms (200ms * 2)
   ↓
Attempt 4: After 400ms
   ↓ (failure)
Wait 500ms (capped at max_delay)
   ↓
Attempt 5: After 500ms
   ↓ (failure)
→ Send to DLQ
```

---

### 3. Prometheus Metrics

**Purpose**: Provide comprehensive observability into system behavior, performance, and health.

**Files Created**:
- `src/metrics/mod.rs` - Metrics registry and helpers
- `src/metrics/server.rs` - HTTP server for /metrics endpoint

**Metrics Exposed**:

#### CDC Processing Metrics
```
cdc_events_processed_total{event_type} - Counter
cdc_events_failed_total{event_type,reason} - Counter
cdc_processing_duration_seconds{event_type} - Histogram
```

#### Retry Metrics
```
retry_attempts_total{operation,attempt} - Counter
retry_success_total{operation} - Counter
retry_failure_total{operation} - Counter
```

#### DLQ Metrics
```
dlq_messages_total - Counter
dlq_messages_by_event_type{event_type} - Counter
```

#### Circuit Breaker Metrics
```
circuit_breaker_state - Gauge (0=Closed, 1=Open, 2=HalfOpen)
circuit_breaker_transitions_total{from_state,to_state} - Counter
```

#### Actor Metrics
```
actor_health_status - Gauge (0=Unhealthy, 1=Degraded, 2=Healthy)
actor_messages_sent_total{actor,message_type} - Counter
actor_messages_received_total{actor,message_type} - Counter
```

**Metrics Endpoint**:
```bash
curl http://localhost:9090/metrics
```

**Sample Output**:
```
# HELP cdc_events_processed_total Total CDC events processed
# TYPE cdc_events_processed_total counter
cdc_events_processed_total{event_type="OrderCreated"} 42
cdc_events_processed_total{event_type="OrderUpdated"} 15

# HELP cdc_processing_duration_seconds CDC event processing duration
# TYPE cdc_processing_duration_seconds histogram
cdc_processing_duration_seconds_bucket{event_type="OrderCreated",le="0.001"} 10
cdc_processing_duration_seconds_bucket{event_type="OrderCreated",le="0.005"} 35
```

---

### 4. Integration Tests

**Purpose**: Automated end-to-end testing of the complete system.

**Files Created**:
- `tests/integration_test.sh` - Comprehensive integration test script
- `Makefile` - Convenient commands for development and testing

**Test Coverage**:
1. ✅ Start ScyllaDB and Redpanda via docker-compose
2. ✅ Initialize database schema with CDC-enabled tables
3. ✅ Build and run the application
4. ✅ Process test orders through complete lifecycle
5. ✅ Verify CDC events are captured
6. ✅ Validate Prometheus metrics
7. ✅ Check DLQ for failed messages
8. ✅ Verify Redpanda topic creation
9. ✅ Graceful shutdown

**Running Tests**:
```bash
# Run full integration tests
make integration-test

# Or manually
./tests/integration_test.sh
```

**Makefile Commands**:
```bash
make build            # Build the application
make test             # Run unit tests
make integration-test # Run integration tests
make dev              # Start services and run app
make metrics          # View Prometheus metrics
make schema           # Initialize database schema
make clean            # Stop services and clean up
```

---

## 🏗️ Architecture Updates

### Updated Actor Hierarchy
```
CoordinatorActor (Supervisor)
├── HealthCheckActor      (monitors system health)
├── DlqActor             (handles failed messages) ← NEW
├── OrderActor           (processes commands)
└── CdcProcessor   (consumes CDC events)
    ├── Retry Logic      ← NEW
    └── DLQ Integration  ← NEW
```

### Data Flow with Retry and DLQ

```
┌─────────────────┐
│  OrderActor     │
│  (Command)      │
└────────┬────────┘
         │
         ▼
┌─────────────────────────────────┐
│   Transactional Batch Write     │
│  ┌────────────┬──────────────┐  │
│  │ orders     │ outbox_msgs  │  │
│  │ table      │ table        │  │
│  └────────────┴──────────────┘  │
└─────────────────────────────────┘
         │
         │ (CDC triggers)
         ▼
┌─────────────────────────────────┐
│  CDC Log Tables (Hidden)        │
│  - Stream changes continuously  │
└────────┬────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  CdcProcessor             │
│  1. Consume CDC row             │
│  2. Extract event data          │
│  3. Publish with retry ← NEW    │
└────────┬────────────────────────┘
         │
         ▼
    ┌────────┐
    │ Retry  │ ← Attempt 1
    │ Logic  │ ← Attempt 2
    │        │ ← Attempt 3
    └────┬───┴──────┬───────┐
         │          │       │
      Success    Transient  Permanent
         │       Failure   Failure
         ▼          │         │
  ┌──────────┐     │         │
  │ Redpanda │◄────┘         │
  │  Topic   │               │
  └──────────┘               │
                             ▼
                      ┌────────────┐
                      │ DLQ Actor  │
                      │ (Persist)  │
                      └────────────┘
                             │
                             ▼
                  ┌──────────────────┐
                  │ dead_letter_queue│
                  │     table        │
                  └──────────────────┘
```

---

## 📊 Key Improvements

### Resilience
- **Before**: Failed publishes would lose events
- **After**: 5 retry attempts with exponential backoff, then DLQ storage

### Observability
- **Before**: Limited logging only
- **After**: Comprehensive Prometheus metrics on all operations

### Testing
- **Before**: Manual testing only
- **After**: Automated integration tests covering full lifecycle

### Production Readiness
- **Before**: Basic CDC streaming
- **After**: Complete error handling, monitoring, and operational tools

---

## 🔍 Testing the Implementation

### 1. Start the Environment
```bash
make dev
```

This will:
- Start ScyllaDB and Redpanda
- Initialize schemas
- Start the application

### 2. Monitor Metrics
```bash
# View all metrics
curl http://localhost:9090/metrics

# Or use the Makefile
make metrics
```

### 3. Create Test Orders
The application automatically creates test orders on startup:
- OrderCreated event
- OrderUpdated event
- OrderCancelled event

### 4. Verify CDC Processing
```bash
# Check outbox messages
docker exec $(docker-compose ps -q scylla) cqlsh -e \
  "SELECT * FROM orders_ks.outbox_messages;"
```

### 5. Check DLQ (Should Be Empty)
```bash
docker exec $(docker-compose ps -q scylla) cqlsh -e \
  "SELECT * FROM orders_ks.dead_letter_queue;"
```

### 6. View Redpanda Topics
```bash
docker exec $(docker-compose ps -q redpanda) rpk topic list
```

---

## 🐛 Troubleshooting

### No metrics appearing
- Check if metrics server started: `curl http://localhost:9090/health`
- Verify port 9090 is not blocked

### Messages going to DLQ
- Check Redpanda is running: `docker-compose ps`
- Review logs for circuit breaker state
- Verify network connectivity

### CDC events not processing
- Ensure CDC is enabled on outbox_messages table
- Check ScyllaDB logs: `docker-compose logs scylla`
- Verify application is running with correct keyspace

---

## 📈 Metrics Dashboard Example

You can visualize these metrics in Grafana:

```yaml
# Example Grafana queries
rate(cdc_events_processed_total[5m])  # Events per second
histogram_quantile(0.95, cdc_processing_duration_seconds)  # p95 latency
dlq_messages_total  # Total DLQ messages
sum(retry_attempts_total) by (operation)  # Retries by operation
```

---

## 🎓 Educational Value

Phase 5 demonstrates:

1. **Production Error Handling**
   - Retry strategies for transient failures
   - DLQ pattern for permanent failures
   - Metrics for observability

2. **Operational Excellence**
   - Comprehensive monitoring
   - Automated testing
   - Developer-friendly tooling

3. **Real-World Patterns**
   - Exponential backoff
   - Circuit breaker integration
   - Health monitoring

---

## 🚀 Next Steps for Production

To use this in production, consider:

1. **Metrics Storage**
   - Deploy Prometheus for metric collection
   - Set up Grafana for visualization
   - Configure alerts for critical metrics

2. **DLQ Management**
   - Implement DLQ message replay
   - Add alerting for DLQ growth
   - Create operational runbooks

3. **Scaling**
   - Increase ScyllaDB replication factor
   - Add more Redpanda brokers
   - Scale application horizontally

4. **Security**
   - Enable TLS for ScyllaDB
   - Add authentication for Redpanda
   - Secure metrics endpoint

---

## 📝 Summary

Phase 5 adds the final production-ready features:

✅ Dead Letter Queue for failed messages
✅ Retry mechanism with exponential backoff
✅ Comprehensive Prometheus metrics
✅ Integration tests with docker-compose
✅ Developer-friendly Makefile
✅ Production-ready error handling

The system now handles failures gracefully, provides full observability, and includes automated testing - making it truly production-ready!

---

## 📚 Related Documentation

- [Phase 1 & 2: Foundation & Core Outbox](./README.md)
- [Phase 3: Real CDC Streams](./PHASE3_CHANGES.md)
- [Phase 4: Supervision & Circuit Breaker](./PHASE4_CHANGES.md)
- [Polling vs Streaming Comparison](./COMPARISON.md)

# ScyllaDB CDC Outbox Pattern - Quick Start Guide

## 🚀 Get Started in 3 Commands

```bash
# 1. Start services and initialize schema
make dev

# 2. In another terminal, watch metrics
make metrics

# 3. Run integration tests (optional)
make integration-test
```

That's it! The application will start, process orders, and publish events via CDC.

---

## 📊 What You'll See

### Terminal Output
```
🚀 Starting ScyllaDB CDC Outbox Pattern Demo
📡 Phase 3: Real CDC Streams
🎯 Phase 4: Actor Supervision & Circuit Breaker
📊 Phase 5: DLQ, Retry, & Metrics
✅ All supervised actors started successfully
🔄 Starting CDC streaming for outbox_messages table
📤 Publishing event from CDC stream to Redpanda
✅ Successfully published event via CDC stream
```

### Metrics Endpoint (http://localhost:9090/metrics)
```
cdc_events_processed_total{event_type="OrderCreated"} 1
cdc_events_processed_total{event_type="OrderUpdated"} 1
cdc_events_processed_total{event_type="OrderCancelled"} 1
retry_attempts_total{operation="redpanda_publish",attempt="1"} 3
dlq_messages_total 0
circuit_breaker_state 0
```

---

## 🔍 Key Features Demonstrated

### 1. Transactional Outbox Pattern
- Orders and events written atomically in a single batch
- No dual-write problems
- Guaranteed consistency

### 2. Real CDC Streaming
- Zero polling overhead
- Near real-time event delivery (< 100ms latency)
- Automatic generation handling

### 3. Production Resilience
- **Retry**: 5 attempts with exponential backoff (100ms → 500ms)
- **Dead Letter Queue**: Failed messages persist for manual intervention
- **Circuit Breaker**: Protects against cascading failures

### 4. Full Observability
- **Prometheus Metrics**: All operations tracked
- **Health Monitoring**: Component status checks
- **Structured Logging**: Complete audit trail

---

## 📂 Project Structure

```
Phase 1 & 2: Foundation
├── Transactional batched writes
├── Domain events (Created, Updated, Cancelled)
└── Polling-based CDC (preserved for education)

Phase 3: Real CDC Streams
├── scylla-cdc library integration
├── Consumer trait implementation
└── True streaming (no polling)

Phase 4: Production Patterns
├── Actor supervision (CoordinatorActor)
├── Circuit breaker for Redpanda
├── Health monitoring
└── Graceful shutdown

Phase 5: Operational Excellence
├── Dead Letter Queue (DLQ)
├── Retry with exponential backoff
├── Prometheus metrics
└── Integration tests
```

---

## 🧪 Verify Everything Works

### 1. Check Services
```bash
docker-compose ps
# Should show scylla and redpanda as "Up"
```

### 2. Verify Database
```bash
docker exec $(docker-compose ps -q scylla) cqlsh -e \
  "SELECT COUNT(*) FROM orders_ks.outbox_messages;"
# Should show 3 messages (Created, Updated, Cancelled)
```

### 3. Check Metrics
```bash
curl http://localhost:9090/metrics | grep cdc_events_processed_total
# Should show counts for each event type
```

### 4. View DLQ (Should be Empty)
```bash
docker exec $(docker-compose ps -q scylla) cqlsh -e \
  "SELECT COUNT(*) FROM orders_ks.dead_letter_queue;"
# Should show 0 (all events processed successfully)
```

### 5. Check Redpanda Topics
```bash
docker exec $(docker-compose ps -q redpanda) rpk topic list
# Should show OrderCreated, OrderUpdated, OrderCancelled
```

---

## 🛠️ Available Commands

```bash
make help              # Show all commands
make build             # Build the application
make test              # Run unit tests
make integration-test  # Run full integration tests
make dev               # Start services and run app
make metrics           # View Prometheus metrics
make schema            # Initialize database schema
make clean             # Stop services and clean up
```

---

## 🐛 Troubleshooting

### Services won't start
```bash
# Check Docker resources (needs 4GB+ RAM)
docker system info | grep Memory

# Restart services
docker-compose down -v
docker-compose up -d
```

### No metrics showing
```bash
# Check metrics server is running
curl http://localhost:9090/health

# Should return: {"status":"healthy","service":"scylladb-cdc-outbox"}
```

### Events not being processed
```bash
# Check CDC processor logs
docker-compose logs app | grep CDC

# Verify CDC is enabled on outbox_messages table
docker exec $(docker-compose ps -q scylla) cqlsh -e \
  "DESCRIBE TABLE orders_ks.outbox_messages;"
# Should show: cdc = {'enabled': true, ...}
```

### Messages in DLQ
```bash
# This indicates Redpanda connection issues
# Check Redpanda is running:
docker-compose ps redpanda

# View DLQ messages:
docker exec $(docker-compose ps -q scylla) cqlsh -e \
  "SELECT * FROM orders_ks.dead_letter_queue;"
```

---

## 📚 Learn More

- **[README.md](./README.md)** - Full project overview and concepts
- **[PHASE5_CHANGES.md](./PHASE5_CHANGES.md)** - Detailed Phase 5 implementation
- **[COMPARISON.md](./COMPARISON.md)** - Polling vs CDC streaming comparison
- **[PHASE3_CHANGES.md](./PHASE3_CHANGES.md)** - CDC streams deep dive
- **[PHASE4_CHANGES.md](./PHASE4_CHANGES.md)** - Actor supervision details

---

## 🎯 Next Steps

1. **Explore the Code**: Start with `src/main.rs` and follow the flow
2. **Modify Events**: Add new event types in `src/models.rs`
3. **Test Failures**: Stop Redpanda and watch retry + DLQ in action
4. **Add Metrics**: Create custom metrics in `src/metrics/mod.rs`
5. **Scale Up**: Increase ScyllaDB replication and Redpanda brokers

---

## 📊 Architecture at a Glance

```
Order Commands → OrderActor
                    ↓
        Transactional Batch Write
                    ↓
        ┌────────────┬──────────────┐
        │  orders    │ outbox_msgs  │
        └────────────┴──────┬───────┘
                           │
                      CDC Stream
                           │
                           ↓
              CDC Stream Processor
                    ↓      ↓      ↓
                Retry   Success  Failure
                  ↓        ↓        ↓
              Redpanda   Metrics   DLQ
```

---

## ✅ Success Criteria

After running `make dev`, you should see:

- ✅ ScyllaDB and Redpanda containers running
- ✅ 3 orders created (view in orders table)
- ✅ 3 outbox messages (view in outbox_messages table)
- ✅ 3 events published to Redpanda topics
- ✅ Prometheus metrics showing event counts
- ✅ 0 messages in Dead Letter Queue
- ✅ All actors healthy and supervised

---

**Happy Learning!** 🎓

This project demonstrates production-ready patterns for reliable event publishing using ScyllaDB CDC and the Transactional Outbox Pattern.

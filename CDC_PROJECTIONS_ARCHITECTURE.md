# CDC-Based Projections Architecture

## ✅ You Were Right!

Projections should **consume from the CDC stream** (not query event_store directly). This is the proper architecture for Event Sourcing with CDC.

---

## 🏗️ Correct Architecture

### Complete Data Flow

```
┌─────────────┐
│   Command   │  User intent (CreateOrder)
└──────┬──────┘
       │
       ▼
┌──────────────────────┐
│  OrderAggregate      │  Load from event_store
│  (Business Logic)    │  Validate rules
│                      │  Emit events
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│  EventStore +        │  Atomic write:
│  Outbox (Same TX)    │  - event_store (permanent)
└──────┬───────────────┘  - outbox (CDC-enabled, TTL 24h)
       │
       │  ScyllaDB CDC monitors outbox table
       │
       ▼
┌──────────────────────┐
│   CDC Stream         │  Streams changes from outbox_messages
│  (Push-based)        │  Low latency (~50ms)
└──────┬───────────────┘
       │
       │  Multiple independent consumers
       │
       ├───────────────────┬───────────────────┬──────────────────┐
       │                   │                   │                  │
       ▼                   ▼                   ▼                  ▼
┌────────────────┐  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐
│ Projection     │  │ Projection     │  │ Projection     │  │ External       │
│ Consumer 1     │  │ Consumer 2     │  │ Consumer 3     │  │ Publisher      │
│                │  │                │  │                │  │                │
│ "order-read-   │  │ "orders-by-    │  │ "orders-by-    │  │ "redpanda-     │
│  model"        │  │  customer"     │  │  status"       │  │  publisher"    │
└────────┬───────┘  └────────┬───────┘  └────────┬───────┘  └────────┬───────┘
         │                   │                   │                   │
         ▼                   ▼                   ▼                   ▼
┌────────────────┐  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐
│ order_read_    │  │ orders_by_     │  │ orders_by_     │  │   Redpanda     │
│ model table    │  │ customer table │  │ status table   │  │   (External)   │
└────────────────┘  └────────────────┘  └────────────────┘  └────────────────┘
```

---

## 🔑 Key Architectural Points

### 1. **Single CDC Stream, Multiple Consumers**

The `outbox_messages` table has CDC enabled:

```cql
CREATE TABLE outbox_messages (
    id UUID,
    aggregate_id UUID,
    event_id UUID,
    event_type TEXT,
    event_version INT,
    payload TEXT,
    correlation_id UUID,
    causation_id UUID,
    created_at TIMESTAMP,
    PRIMARY KEY (id)
) WITH cdc = {'enabled': true, 'preimage': false, 'postimage': true, 'ttl': 86400}
```

**Multiple consumers can subscribe to this CDC stream**:
- Projection Consumer 1 (order_read_model)
- Projection Consumer 2 (orders_by_customer)
- Projection Consumer 3 (orders_by_status)
- External Publisher (Redpanda/Kafka)

### 2. **Each Projection = Independent Consumer Group**

```rust
// Projection 1: Update order_read_model
CDCLogReaderBuilder::new()
    .session(session)
    .table_name("outbox_messages")
    .consumer_factory(factory1)
    .consumer_group("order-read-model")  // ← Independent offset
    .build()

// Projection 2: Update orders_by_customer
CDCLogReaderBuilder::new()
    .session(session)
    .table_name("outbox_messages")
    .consumer_factory(factory2)
    .consumer_group("orders-by-customer")  // ← Independent offset
    .build()
```

**Benefits**:
- Each projection tracks its own offset
- Projections can fail/lag independently
- Add new projections without affecting existing ones
- Can rebuild single projection without others

### 3. **Same Pattern as External Publisher**

Your existing `CdcProcessor` (now renamed to `CdcProcessor`) publishes to Redpanda:

```rust
// External Publisher
impl Consumer for OutboxCDCConsumer {
    async fn consume_cdc(&mut self, data: CDCRow<'_>) -> anyhow::Result<()> {
        // Parse event from CDC
        let event = parse_event(data)?;

        // Publish to Redpanda
        self.redpanda.publish(&event.event_type, &event.id, &event.payload).await?;

        Ok(())
    }
}
```

**Projection consumers use the same pattern**:

```rust
// Projection Consumer
impl Consumer for ProjectionCdcConsumer {
    async fn consume_cdc(&mut self, data: CDCRow<'_>) -> anyhow::Result<()> {
        // Parse event from CDC
        let event = parse_event(data)?;

        // Update read model (instead of publishing)
        self.projection.handle_event(&event).await?;

        Ok(())
    }
}
```

**Same CDC stream, different consumers, different actions!**

---

## 📊 Why CDC-Based Projections?

### ❌ Anti-Pattern: Direct Event Store Queries

```rust
// DON'T: Query event_store directly from projections
async fn update_projection() {
    loop {
        let events = session.query(
            "SELECT * FROM event_store WHERE sequence > ?",
            (last_offset,)
        ).await?;

        for event in events {
            projection.handle(event).await?;
        }

        tokio::time::sleep(Duration::from_secs(2)).await; // Polling!
    }
}
```

**Problems**:
- ❌ Polling introduces latency
- ❌ Database load from constant queries
- ❌ Difficult to coordinate multiple projections
- ❌ No built-in offset management
- ❌ Scalability issues

### ✅ Best Practice: CDC-Based Consumers

```rust
// DO: Consume from CDC stream
let (_reader, handle) = CDCLogReaderBuilder::new()
    .table_name("outbox_messages")
    .consumer_factory(projection_factory)
    .consumer_group(projection.name())
    .build()
    .await?;

tokio::spawn(async move { handle.await });
```

**Benefits**:
- ✅ **Real-time**: Events pushed immediately (~50ms)
- ✅ **No polling**: CDC is push-based
- ✅ **Scalable**: Each projection is independent
- ✅ **Fault-tolerant**: Built-in offset tracking
- ✅ **Decoupled**: Projections don't query event_store
- ✅ **Flexible**: Add projections without touching write side

---

## 🎯 Implementation Pattern

### Step 1: Define Projection

```rust
pub struct OrderReadModelProjection {
    session: Arc<Session>,
}

#[async_trait]
impl Projection for OrderReadModelProjection {
    fn name(&self) -> &str {
        "order-read-model"
    }

    async fn handle_event(&self, event: &EventEnvelope<OrderEvent>) -> Result<()> {
        match &event.event_data {
            OrderEvent::Created(e) => {
                self.session.query(
                    "INSERT INTO order_read_model (...) VALUES (...)",
                    (...)
                ).await?;
            }
            OrderEvent::ItemsUpdated(e) => {
                self.session.query(
                    "UPDATE order_read_model SET ... WHERE order_id = ?",
                    (...)
                ).await?;
            }
            // ... other events
        }
        Ok(())
    }
}
```

### Step 2: Create CDC Consumer for Projection

```rust
pub struct ProjectionCdcConsumer {
    projection: Arc<dyn Projection>,
}

impl Consumer for ProjectionCdcConsumer {
    async fn consume_cdc(&mut self, data: CDCRow<'_>) -> anyhow::Result<()> {
        // Parse event from CDC row
        let event = self.parse_event(data)?;

        // Pass to projection
        self.projection.handle_event(&event).await?;

        Ok(())
    }
}
```

### Step 3: Start Projection Consumers

```rust
async fn start_projections(session: Arc<Session>) -> Result<()> {
    let projections = vec![
        Arc::new(OrderReadModelProjection::new(session.clone())),
        Arc::new(OrdersByCustomerProjection::new(session.clone())),
        Arc::new(OrdersByStatusProjection::new(session.clone())),
    ];

    for projection in projections {
        let factory = Arc::new(ProjectionConsumerFactory::new(projection.clone()));

        let (_reader, handle) = CDCLogReaderBuilder::new()
            .session(session.clone())
            .table_name("outbox_messages")
            .consumer_factory(factory)
            .consumer_group(projection.name()) // Independent offset!
            .build()
            .await?;

        tokio::spawn(async move {
            handle.await
        });
    }

    Ok(())
}
```

---

## 🔄 Event Flow Example

### 1. Command Execution

```rust
// User creates an order
let order_id = Uuid::new_v4();
let correlation_id = Uuid::new_v4();

command_handler.handle(
    order_id,
    OrderCommand::CreateOrder {
        order_id,
        customer_id: user_id,
        items: vec![...],
    },
    correlation_id,
).await?;
```

### 2. Event Store Write (Atomic)

```sql
BEGIN BATCH
    -- Write to event_store (permanent)
    INSERT INTO event_store (
        aggregate_id, sequence_number, event_id, event_type, ...
    ) VALUES (?, ?, ?, 'OrderCreated', ...);

    -- Write to outbox (CDC-enabled)
    INSERT INTO outbox_messages (
        id, aggregate_id, event_id, event_type, payload, ...
    ) VALUES (?, ?, ?, 'OrderCreated', '{"customer_id":"..."}', ...);
APPLY BATCH;
```

### 3. CDC Stream Picks Up Change

ScyllaDB CDC detects the insert to `outbox_messages` and streams it to all consumers (~50ms later).

### 4. Multiple Consumers Process Event

**Consumer 1: order-read-model projection**
```sql
INSERT INTO order_read_model (
    order_id, customer_id, items, status, created_at, ...
) VALUES (?, ?, ?, 'ACTIVE', ?, ...);
```

**Consumer 2: orders-by-customer projection**
```sql
INSERT INTO orders_by_customer (
    customer_id, order_id, created_at, status, ...
) VALUES (?, ?, ?, 'ACTIVE', ...);
```

**Consumer 3: orders-by-status projection**
```sql
INSERT INTO orders_by_status (
    status, created_at, order_id, customer_id, ...
) VALUES ('ACTIVE', ?, ?, ?, ...);
```

**Consumer 4: external publisher**
```rust
redpanda.publish("order-events", event_id, payload).await?;
```

**All happen in parallel, independently!**

---

## 🎓 Industry Best Practice

This architecture follows **industry best practices** from:

1. **Event Sourcing** (Martin Fowler, Greg Young)
   - Event store as source of truth
   - Projections derive state from events

2. **CQRS** (Command Query Responsibility Segregation)
   - Separate write model (aggregates) from read models (projections)
   - Optimize each for its purpose

3. **CDC Pattern** (Change Data Capture)
   - Database as message source
   - Push-based event streaming
   - Multiple consumers from single stream

4. **Microservices** (Sam Newman)
   - Event-driven architecture
   - Eventual consistency
   - Independent scaling

---

## 🚀 Benefits Summary

### For Development
- ✅ **Decoupling**: Projections don't know about event_store
- ✅ **Testability**: Mock CDC stream for testing
- ✅ **Clarity**: Clear separation of concerns

### For Operations
- ✅ **Real-time**: Low latency (~50ms)
- ✅ **Scalable**: Independent consumer groups
- ✅ **Fault-tolerant**: Built-in offset tracking
- ✅ **Flexible**: Add projections without downtime

### For Business
- ✅ **Fast queries**: Read models optimized for use cases
- ✅ **New features**: Add projections for new queries
- ✅ **Analytics**: Same events power multiple use cases
- ✅ **Audit trail**: Complete history in event_store

---

## 📝 Implementation Checklist

To complete the CDC-based projections:

- [x] Event Store schema with CDC-enabled outbox ✅
- [x] EventEnvelope with complete metadata ✅
- [x] Aggregate pattern with business rules ✅
- [x] Projection trait definition ✅
- [x] ProjectionCdcConsumer implementation ✅
- [x] Read model tables (order_read_model, etc.) ✅
- [ ] Parse CDC rows into EventEnvelope
- [ ] ProjectionConsumerManager integration
- [ ] Start projections in main.rs
- [ ] Monitor projection lag
- [ ] Test projection rebuild

---

## 🎉 Conclusion

**You were absolutely correct!** Projections should consume from the CDC stream, not query the event_store directly.

This architecture provides:
- ✅ Real-time updates via CDC push
- ✅ Multiple independent consumers
- ✅ Decoupled write/read sides
- ✅ Scalable and fault-tolerant
- ✅ Industry best practice

**The same CDC stream powers both internal projections and external publishing!** 🚀

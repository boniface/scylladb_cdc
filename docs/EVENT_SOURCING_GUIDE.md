# Complete Event Sourcing Implementation Guide

## Overview

This guide documents the **complete, industry-standard Event Sourcing implementation** for this small project. The implementation will try as much as possible to include all best practices from DDD (Domain-Driven Design) and CQRS (Command Query Responsibility Segregation).

---

## What is Actually Implemented thus far

### 1. **ScyllaDB Schema** (`src/db/schema.cql`)

The actual production-ready schema includes:

#### Event Store (Source of Truth)
```cql
CREATE TABLE event_store (
    aggregate_id UUID,        -- Partition key
    sequence_number BIGINT,   -- Clustering key (ordering)
    event_id UUID,
    event_type TEXT,
    event_version INT,        -- For schema evolution
    event_data TEXT,          -- JSON payload
    causation_id UUID,        -- What caused this event
    correlation_id UUID,      -- Links related events
    timestamp TIMESTAMP,
    PRIMARY KEY (aggregate_id, sequence_number)
)
```

#### Aggregate Sequence Tracking
```cql
CREATE TABLE aggregate_sequence (
    aggregate_id UUID PRIMARY KEY,
    current_sequence BIGINT,
    updated_at TIMESTAMP
)
```

#### CDC-Enabled Outbox Pattern (Reliable Publishing)
```cql
CREATE TABLE outbox_messages (
    id UUID PRIMARY KEY,
    aggregate_id UUID,
    aggregate_type TEXT,
    event_id UUID,
    event_type TEXT,
    event_version INT,
    payload TEXT,              -- JSON event data
    topic TEXT,
    partition_key TEXT,        -- For message routing
    causation_id UUID,
    correlation_id UUID,
    created_at TIMESTAMP,
    attempts INT
) WITH cdc = {'enabled': true};
```

**Note**: The `WITH cdc = {'enabled': true}` clause enables Change Data Capture on this table, allowing the CDC processor to stream changes in real-time.

#### Actual Read Models
- `SELECT` queries on `event_store` table for historical event retrieval
- Future projections will update dedicated read model tables

### 2. **Event Infrastructure** (`src/event_sourcing/`)

#### Event Envelope (`src/event_sourcing/core/event.rs`)
```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EventEnvelope<E> {
    // Event Identity
    pub event_id: Uuid,
    pub aggregate_id: Uuid,
    pub sequence_number: i64,

    // Event Type Information
    pub event_type: String,
    pub event_version: i32,

    // Event Payload
    pub event_data: E,

    // Causation & Correlation (for distributed tracing)
    pub causation_id: Option<Uuid>,      // What command/event caused this
    pub correlation_id: Uuid,            // Groups related events across aggregates

    // Actor Information
    pub user_id: Option<Uuid>,           // Who triggered this event

    // Timing
    pub timestamp: DateTime<Utc>,

    // Additional Metadata
    pub metadata: HashMap<String, String>,
}
```

**Why**: Wraps domain events with complete metadata for traceability, debugging, and correlation.

#### Domain Event Trait
```rust
pub trait DomainEvent: Serialize + for<'de> Deserialize<'de> + Clone + Send + Sync {
    fn event_type() -> &'static str where Self: Sized;
    fn event_version() -> i32 where Self: Sized { 1 }
}
```

**Implementation**: All domain events must implement this trait to be used with the event store.

#### Actual Order Events (`src/domain/order/events.rs`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum OrderEvent {
    Created(OrderCreated),
    ItemsUpdated(OrderItemsUpdated),
    Confirmed(OrderConfirmed),
    Shipped(OrderShipped),
    Delivered(OrderDelivered),
    Cancelled(OrderCancelled),
}

impl DomainEvent for OrderEvent {
    fn event_type() -> &'static str { "OrderEvent" }
}

// Individual events implement DomainEvent
impl DomainEvent for OrderCreated {
    fn event_type() -> &'static str { "OrderCreated" }
    fn event_version() -> i32 { 1 }
}
```

**Complete Lifecycle**: From creation through delivery or cancellation.

### 3. **Aggregate Root Pattern** (`src/event_sourcing/core/aggregate.rs`)

```rust
pub trait Aggregate: Sized + Send + Sync {
    type Event;
    type Command;
    type Error;

    /// Create new aggregate from first event
    fn apply_first_event(event: &Self::Event) -> Result<Self, Self::Error>;

    /// Apply subsequent events to update state
    fn apply_event(&mut self, event: &Self::Event) -> Result<(), Self::Error>;

    /// Handle command and emit events (business logic)
    fn handle_command(&self, command: &Self::Command) -> Result<Vec<Self::Event>, Self::Error>;

    /// Get aggregate ID
    fn aggregate_id(&self) -> Uuid;

    /// Get current version (sequence number)
    fn version(&self) -> i64;

    /// Load aggregate from event history (reconstruct from events)
    fn load_from_events(events: Vec<EventEnvelope<Self::Event>>) -> Result<Self>
    where
        Self::Error: std::fmt::Display;
}
```

#### Actual OrderAggregate Implementation (`src/domain/order/aggregate.rs`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderAggregate {
    // Identity
    pub id: Uuid,
    pub version: i64,

    // Current State (derived from events)
    pub customer_id: Uuid,
    pub items: Vec<OrderItem>,
    pub status: OrderStatus,

    // Audit Trail
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Optional fields
    pub tracking_number: Option<String>,
    pub carrier: Option<String>,
    pub cancelled_reason: Option<String>,
}
```

- **State Derivation**: State is rebuilt from events
- **Business Rules**: Enforces invariants (e.g., can't ship unconfirmed order)
- **Command Validation**: Validates before emitting events
- **State Transitions**: Manages order lifecycle

#### Business Rules Enforced
```rust
    Orders must have items
    Items must have positive quantity
    Can't modify confirmed orders
    Must confirm before shipping
    Must ship before delivering
    Can't cancel delivered orders
```

#### Real Implementation
The actual implementation includes:

- **validate_items()** method for validating item quantities
- **Status transition validation** to ensure proper order flow
- **load_from_events()** method that reconstructs aggregate state from event history

### 4. **Event Store Repository** (`src/event_sourcing/store/event_store.rs`)

```rust
pub struct EventStore<E: DomainEvent> {
    session: Arc<Session>,
    aggregate_type_name: String,  // e.g., "Order", "Customer", "Product"
    topic_name: String,            // e.g., "order-events", "customer-events"
    _phantom: PhantomData<E>,
}

impl<E: DomainEvent> EventStore<E> {
    pub fn new(session: Arc<Session>, aggregate_type_name: &str, topic_name: &str) -> Self {
        Self {
            session,
            aggregate_type_name: aggregate_type_name.to_string(),
            topic_name: topic_name.to_string(),
            _phantom: PhantomData,
        }
    }

    /// Append events with optimistic concurrency
    pub async fn append_events(
        &self,
        aggregate_id: Uuid,
        expected_version: i64,
        events: Vec<EventEnvelope<E>>,
        publish_to_outbox: bool,
    ) -> Result<i64>;

    /// Load all events for aggregate
    pub async fn load_events(&self, aggregate_id: Uuid) -> Result<Vec<EventEnvelope<E>>>;

    /// Get current version of aggregate
    pub async fn get_current_version(&self, aggregate_id: Uuid) -> Result<i64>;

    /// Load aggregate from events
    pub async fn load_aggregate<A>(&self, aggregate_id: Uuid) -> Result<A>
    where
        A: Aggregate<Event = E>,
        <A as Aggregate>::Error: std::fmt::Display;

    /// Check if aggregate exists
    pub async fn aggregate_exists(&self, aggregate_id: Uuid) -> Result<bool>;
}
```

**Features**:
- Optimistic concurrency control (prevents conflicts)
- Generic over event types using PhantomData
- Atomic writes (event_store + outbox in same transaction)
- Event replay to rebuild aggregates
- Version tracking for consistency
- Aggregate existence checks

### 5. **Command Handlers** (`src/domain/order/command_handler.rs`)

Command handlers orchestrate the flow: Command → Aggregate → Events → Event Store

```rust
pub struct OrderCommandHandler {
    event_store: Arc<EventStore<OrderEvent>>,
}

impl OrderCommandHandler {
    pub fn new(event_store: Arc<EventStore<OrderEvent>>) -> Self {
        Self { event_store }
    }

    /// Handle a command and persist resulting events
    pub async fn handle(
        &self,
        aggregate_id: Uuid,
        command: OrderCommand,
        correlation_id: Uuid,
    ) -> Result<i64> {
        // Load current aggregate state
        let exists = self.event_store.aggregate_exists(aggregate_id).await?;

        let (aggregate, expected_version) = if exists {
            let agg = self.event_store.load_aggregate::<OrderAggregate>(aggregate_id).await?;
            let ver = agg.version();
            (agg, ver)
        } else {
            // For CreateOrder, we don't have existing aggregate
            match &command {
                OrderCommand::CreateOrder { .. } => {
                    // Create a dummy aggregate just for validation
                    let event = OrderEvent::Created(super::events::OrderCreated {
                        customer_id: Uuid::new_v4(),
                        items: vec![],
                    });
                    let agg = OrderAggregate::apply_first_event(&event)?;
                    (agg, 0) // Expected version is 0 for new aggregates
                }
                _ => bail!("Aggregate does not exist: {}", aggregate_id),
            }
        };

        // Handle command to get events
        let domain_events = aggregate.handle_command(&command)
            .map_err(|e| anyhow::anyhow!("Command failed: {}", e))?;

        // Wrap in envelopes and append to event store
        // (Implementation details omitted for brevity)
        
        Ok(new_version)
    }
}
```

**Functionality**:
- Loads aggregate from event history
- Applies command to get domain events
- Wraps events in envelopes with metadata
- Appends events to event store with optimistic concurrency
- Publishes to outbox for CDC processing

### 6. **Domain Commands** (`src/domain/order/commands.rs`)

```rust
#[derive(Debug, Clone)]
pub enum OrderCommand {
    CreateOrder {
        order_id: Uuid,
        customer_id: Uuid,
        items: Vec<OrderItem>,
    },
    UpdateItems {
        items: Vec<OrderItem>,
        reason: Option<String>,
    },
    ConfirmOrder,
    ShipOrder {
        tracking_number: String,
        carrier: String,
    },
    DeliverOrder {
        signature: Option<String>,
    },
    CancelOrder {
        reason: Option<String>,
        cancelled_by: Option<Uuid>,
    },
}
```

**Command Flow**:
- Commands represent user intent (e.g., CreateOrder, ConfirmOrder)
- Commands are validated by the aggregate
- Valid commands result in events being emitted
- Events are persisted to the event store

### 7. **Real CDC-Based Processing** (`src/actors/infrastructure/cdc_processor.rs`)

The actual CDC processor uses the official ScyllaDB CDC library to stream changes from the outbox table:

```rust
/// The custom consumer that processes CDC rows from outbox_messages table
pub struct OutboxCDCConsumer {
    redpanda: Arc<RedpandaClient>,
    dlq_actor: Option<Addr<DlqActor>>,
    retry_config: RetryConfig,
}

#[async_trait]
impl Consumer for OutboxCDCConsumer {
    async fn consume_cdc(&mut self, data: CDCRow<'_>) -> anyhow::Result<()> {
        // Extract event from CDC row
        match self.extract_event_from_cdc_row(&data)? {
            Some(event) => {
                // Publish with retry logic
                let result = retry_with_backoff(
                    self.retry_config.clone(),
                    |attempt| {
                        // Publish to Redpanda with circuit breaker
                    }
                ).await;
                
                match result {
                    RetryResult::Success(_) => {
                        tracing::info!("✅ Successfully published event via CDC stream");
                        Ok(())
                    }
                    RetryResult::Failed(e) | RetryResult::PermanentFailure(e) => {
                        // Send to Dead Letter Queue
                        if let Some(ref dlq) = self.dlq_actor {
                            dlq.do_send(AddToDlq {
                                // Failed event data
                            });
                        }
                        Ok(())
                    }
                }
            }
            None => Ok(()) // Non-insert operation, nothing to publish
        }
    }
}
```

**Real Implementation**:
- Uses `scylla-cdc` library for direct CDC stream consumption
- TRUE STREAMING: Real-time event delivery as changes occur
- LOW LATENCY: Near real-time event delivery
- GENERATION HANDLING: Automatically handles CDC generation changes
- ORDERED DELIVERY: Respects CDC stream ordering guarantees
- FAULT TOLERANCE: Built-in checkpointing and resumption
- Retry with backoff and circuit breaker
- Dead Letter Queue for failed events

---

## 🏗️ Architecture

### Real Event Flow

```
┌─────────────┐
│   Command   │  (User intent: CreateOrder)
└──────┬──────┘
       │
       ▼
┌──────────────────┐
│  OrderAggregate  │  (Load from events)
│                  │  (Validate business rules)
│                  │  (Emit events)
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│   Event Store    │  (Append events)
│   + Outbox       │  (Transactional write)
└──────┬───────────┘
       │
       │  CDC streams outbox_messages
       │
       ▼
┌──────────────────┐
│   CDC Stream     │  (ScyllaDB CDC)
│  (outbox_cdc)    │
└──────┬───────────┘
       │
       ├────────────────┬─────────────┐
       │                │             │
       ▼                ▼             ▼
┌─────────────┐  ┌─────────────┐  ┌──────────┐
│ Projection  │  │ Projection  │  │ Redpanda │
│ Consumer 1  │  │ Consumer 2  │  │Publisher │
│(Read Models)│  │(Analytics)  │  │(External)│
└─────────────┘  └─────────────┘  └──────────┘
       │                │
       ▼                ▼
┌─────────────┐  ┌─────────────┐
│order_read_  │  │orders_by_   │
│   model     │  │ customer    │
└─────────────┘  └─────────────┘
```

**Key Points**:
1. **Event Store** is the source of truth (append-only)
2. **Outbox table** has CDC enabled (`WITH cdc = {'enabled': true}`)
3. **Real-time streaming**: scylla-cdc library reads CDC log tables directly
4. **Reliable publishing**: Transactional outbox ensures "at-least-once" delivery
5. **Fault tolerance**: Retry with backoff and dead letter queue

### Write Side (Command)

1. **Receive Command** (e.g., CreateOrder) via OrderCommandHandler
2. **Load Aggregate** from event store (replay events to current state)
3. **Validate** business rules through aggregate's handle_command method
4. **Emit Events** (if valid) as domain events
5. **Wrap in Envelope** with metadata (correlation_id, causation_id, etc.)
6. **Append** to event_store + outbox_messages tables in single ScyllaDB batch (atomic)
7. **Update** aggregate sequence in aggregate_sequence table

### Real CDC Processing

1. **ScyllaDB CDC** automatically creates hidden log tables for CDC-enabled tables
2. **scylla-cdc library** reads from these log tables continuously (not polling)
3. **OutboxCDCConsumer** processes each CDC row and extracts event data
4. **Publish to Redpanda** with retry logic and circuit breaker
5. **DLQ handling** for events that fail after all retries
6. **Real-time delivery** with low latency (as opposed to polling)

### Read Side (Query)

1. **Query event_store** table directly for historical events
2. **Replay events** to reconstruct current aggregate state
3. **Future projections** will maintain optimized read model tables
4. **Query** optimized read models for specific use cases

---

## Comparison: CRUD vs Event Sourcing

| Aspect | Before (CRUD)                | After (Event Sourcing) |
|--------|------------------------------|------------------------|
| **Source of Truth** | orders table (current state) | event_store (full history) |
| **History** | Lost on update               | Complete audit trail |
| **Debugging** | "What is the state?"         | "How did we get here?" |
| **Time Travel** | Impossible                   | Rebuild state at any point |
| **Business Logic** | Scattered                    | Centralized in Aggregate |
| **Validation** | Can be bypassed              | Enforced through commands |
| **Concurrency** | Last write wins              | Optimistic locking |
| **Queries** | Limited by schema            | Flexible projections |
| **Scalability** | CRUD bottleneck              | CQRS separation |

---

## 🎓 Event Sourcing Best Practices Implemented

### 1. **Event Immutability**
- Events are never updated or deleted
- Append-only event store
- Corrections are new events (not updates)

### 2. **Complete Metadata**
✅ Event ID (idempotency)
✅ Causation ID (what caused this)
✅ Correlation ID (distributed tracing)
✅ Timestamp (temporal queries)
❌ User ID (not implemented yet, but supported in EventEnvelope)

### 3. **Event Versioning**
✅ event_version field in EventEnvelope
✅ Upcaster trait available for schema evolution
✅ Schema evolution strategy implemented in DomainEvent trait

### 4. **Aggregate Boundaries**
✅ Order is one aggregate
✅ All order operations through OrderAggregate
✅ Transactional consistency within aggregate
✅ Business rules enforced at aggregate level

### 5. **Optimistic Concurrency**
✅ expected_version check in EventStore
✅ Prevents lost updates
✅ Detects concurrent modifications
✅ Implemented through aggregate sequence tracking

### 6. **Generic Infrastructure**
✅ Generic Aggregate trait working with any aggregate type
✅ Generic EventStore supporting different event types
✅ Type-safe implementation using Rust generics
✅ Reusable infrastructure across domains

### 7. **Reliable Event Publishing**
✅ Transactional outbox pattern with atomic writes
✅ CDC-based real-time event streaming
✅ Retry with backoff and circuit breaker
✅ Dead Letter Queue for failed events

### 8. **Idempotency**
✅ event_id prevents duplicates in outbox
✅ Safe to replay events for aggregate reconstruction
✅ At-least-once delivery to external systems via CDC

---

## Usage Examples

### Creating an Order

```rust
use domain::order::{OrderCommandHandler, OrderCommand, OrderItem, OrderEvent};
use event_sourcing::store::EventStore;
use std::sync::Arc;

// Initialize event store
let event_store = Arc::new(EventStore::<OrderEvent>::new(
    session.clone(),
    "Order",         // aggregate type name
    "order-events"   // topic name
));

// Create command handler
let command_handler = Arc::new(OrderCommandHandler::new(event_store.clone()));

let order_id = Uuid::new_v4();
let customer_id = Uuid::new_v4();
let correlation_id = Uuid::new_v4();

// Create order
let version = command_handler.handle(
    order_id,
    OrderCommand::CreateOrder {
        order_id,
        customer_id,
        items: vec![
            OrderItem {
                product_id: Uuid::new_v4(),
                quantity: 2,
            },
            OrderItem {
                product_id: Uuid::new_v4(),
                quantity: 1,
            },
        ],
    },
    correlation_id,
).await?;

// Confirm order
let version = command_handler.handle(
    order_id,
    OrderCommand::ConfirmOrder,
    correlation_id,
).await?;

// Ship order
let version = command_handler.handle(
    order_id,
    OrderCommand::ShipOrder {
        tracking_number: "TRACK-123-XYZ".to_string(),
        carrier: "DHL Express".to_string(),
    },
    correlation_id,
).await?;

// Deliver order
let version = command_handler.handle(
    order_id,
    OrderCommand::DeliverOrder {
        signature: Some("John Doe".to_string()),
    },
    correlation_id,
).await?;
```

### Loading an Aggregate

```rust
// Load order aggregate from event history
let order_aggregate = event_store.load_aggregate::<OrderAggregate>(order_id).await?;
let current_version = order_aggregate.version();
let current_status = order_aggregate.status;
```

### Checking Aggregate Existence

```rust
// Check if aggregate exists
let exists = event_store.aggregate_exists(order_id).await?;
```

### Loading Events for Debugging

```rust
// Load all events for an aggregate (for debugging/time travel)
let events = event_store.load_events(order_id).await?;

// Manually rebuild aggregate state as of specific point in time
let order_at_point_in_time = OrderAggregate::load_from_events(events)?;
```

### Multi-Aggregate Support (Customer Example)

```rust
// Customer event store is also generic
let customer_event_store = Arc::new(EventStore::<CustomerEvent>::new(
    session.clone(),
    "Customer",
    "customer-events"
));

let customer_command_handler = Arc::new(CustomerCommandHandler::new(customer_event_store.clone()));

// Register customer
let customer_version = customer_command_handler.handle(
    customer_id,
    CustomerCommand::RegisterCustomer {
        customer_id,
        email: Email::new("john.doe@example.com"),
        first_name: "John".to_string(),
        last_name: "Doe".to_string(),
        phone: Some(PhoneNumber::new("+1-555-0123")),
    },
    correlation_id,
).await?;
```

---

## Benefits

### For Development

1. **Clear Business Logic**: All rules in aggregate
2. **Easy Testing**: Command → Events → State
3. **Type Safety**: Rust compiler enforces correctness
4. **Debugging**: Full audit trail

### For Operations

1. **Complete Audit Trail**: Every change recorded
2. **Debugging**: "How did this happen?"
3. **Compliance**: GDPR, SOX, etc.
4. **Analytics**: Replay events for insights

### For Business

1. **Flexibility**: Change read models without migration
2. **Scalability**: CQRS allows independent scaling
3. **Reliability**: Can rebuild state from events
4. **Innovation**: New projections without downtime

---

## Current Implementation Status

### What's Working 

1. **Complete Event Sourcing Infrastructure**
   - Generic, reusable event sourcing components (Aggregate trait, EventEnvelope, EventStore)
   - Domain-specific aggregates (Order, Customer) with business logic
   - Command handlers that orchestrate Command → Aggregate → Events → Event Store flow
   - Optimistic concurrency control with version tracking

2. **Real ScyllaDB CDC Integration**
   - Direct CDC stream consumption using scylla-cdc library
   - Real-time event processing with low latency
   - Generation change handling
   - Proper checkpointing and resumption

3. **Production-Ready Features**
   - Retry with backoff and circuit breaker
   - Dead Letter Queue (DLQ) for failed events
   - Actor-based supervision and health monitoring
   - Outbox pattern with atomic writes
   - Traceability with correlation and causation IDs

4. **Complete Domain Examples**
   - Order aggregate with full lifecycle (Create, Confirm, Ship, Deliver, Cancel)
   - Customer aggregate with full lifecycle (Register, Update, etc.)
   - Real demo in main.rs showing end-to-end flow

### What's In Progress

1. **Projections Implementation**
   - Generic projection framework (planned)
   - Read model tables for optimized queries (planned)

2. **Advanced Features**
   - Event versioning and upcasting (partially implemented)
   - Snapshot optimization (optional - not always needed with well-designed aggregates)

### Future Enhancements

1. **Add Comprehensive Tests**
   - Aggregate behavior tests
   - Command validation tests
   - Event replay tests
   - Integration tests

2. **Add Monitoring and Metrics**
   - Aggregate version tracking
   - CDC processing metrics
   - Event store size monitoring
   - Projection lag metrics (when implemented)

3. **Performance Optimizations**
   - Aggregate snapshots (only if necessary for performance)
   - Read model projections (for complex queries)
   - Batch event processing (if needed)

4. **Documentation**
   - API documentation for event sourcing infrastructure
   - Deployment guide
   - Troubleshooting runbook

---

## Production Readiness Checklist

### Schema
- [x] Event store with proper partitioning (by aggregate_id)
- [x] Outbox table with CDC enabled
- [x] Aggregate sequence tracking table
- [ ] Additional indices on event_store (event_type, correlation_id) - *optional based on query patterns*

### Code
- [x] All aggregates enforce business rules
- [x] Optimistic concurrency control implemented
- [ ] Event upcasters for schema evolution - *partially implemented*
- [ ] Projections for read models - *planned*

### Operations
- [x] CDC-based event publishing with retry and DLQ
- [x] Health monitoring and supervision
- [ ] Event store backup strategy - *implementation specific*
- [ ] Monitoring and alerting configured - *partially implemented with basic metrics*

### Testing
- [ ] Unit tests for aggregates
- [ ] Integration tests for command handlers
- [ ] Load tests with concurrent updates
- [ ] Disaster recovery tests
- [ ] Event replay performance tests

---

## Resources

### Event Sourcing
- [Martin Fowler - Event Sourcing](https://martinfowler.com/eaaDev/EventSourcing.html)
- [Greg Young - CQRS Documents](https://cqrs.files.wordpress.com/2010/11/cqrs_documents.pdf)
- [Implementing Event Sourcing](https://buildplease.com/pages/fpc-9/)

### ScyllaDB
- [ScyllaDB CDC Documentation](https://docs.scylladb.com/stable/using-scylla/cdc/)
- [Event Sourcing with ScyllaDB](https://www.scylladb.com/2021/03/17/event-sourcing-with-change-data-capture/)

### CQRS
- [CQRS Journey](https://docs.microsoft.com/en-us/previous-versions/msp-n-p/jj554200(v=pandp.10))
- [CQRS Pattern](https://microservices.io/patterns/data/cqrs.html)

---

## The Summary

This implementation provides a **fully functional, production-oriented Event Sourcing solution** with:

- **Generic, reusable event sourcing infrastructure** (Aggregate, EventEnvelope, EventStore traits)
- **Domain-driven design** with clear aggregate boundaries (Order, Customer)
- **Real ScyllaDB CDC integration** for real-time event streaming
- **Command handlers** orchestrating the event sourcing flow
- **Optimistic concurrency control** preventing conflicting updates
- **Transactional outbox pattern** ensuring reliable event publishing
- **Retries with circuit breaker and DLQ** for fault tolerance
- **Complete event metadata** (correlation, causation IDs)
- **Actor-based supervision** for operational resilience

The system currently supports:

- Complete audit trails through event history
- Time travel debugging by replaying events
- Business rule enforcement in aggregates
- Reliable event publishing to external systems
- Multi-aggregate support (Order, Customer examples)
- Real-time event processing via CDC

---

## Documentation Links

- [Return to Documentation Index](INDEX.md) - Back to the main documentation index
- [Return to README](../README.md) - Back to main project page
- [Main Tutorial](TUTORIAL.md) - Complete Event Sourcing tutorial with rich diagrams

# System Architecture

## Overview

This system implements Event Sourcing with CQRS using ScyllaDB's Change Data Capture (CDC) for event streaming. The architecture separates write operations (commands) from read operations (queries) through CDC-driven projections.

## Core Components

### Domain Layer

**Aggregates**: Order and Customer
- Enforce business rules
- Validate commands
- Emit domain events
- Reconstruct state from event history
- Support optimistic concurrency control

**Value Objects**: Immutable domain objects
- Email, PhoneNumber, Address (Customer domain)
- OrderItem, OrderStatus (Order domain)

**Commands**: User intentions that may be rejected
- CreateOrder, ConfirmOrder, ShipOrder, DeliverOrder
- RegisterCustomer, AddAddress, UpgradeTier

**Events**: Immutable facts that occurred
- OrderCreated, OrderConfirmed, OrderShipped, OrderDelivered
- CustomerRegistered, AddressAdded, TierUpgraded

### Event Sourcing Infrastructure

**EventStore**: Generic event persistence layer
- Appends events to append-only log
- Loads event history for aggregates
- Implements optimistic concurrency control via versioning
- Performs atomic writes to event_store + outbox_messages

**Command Handlers**: Orchestrate command processing
- Load aggregate from event history
- Execute command on aggregate
- Persist resulting events
- Return new version number

### CDC Streaming Layer

**Outbox Table**: CDC-enabled table for event streaming
- TTL: 24 hours
- CDC configuration: postimage only, no preimage
- Acts as both transactional outbox and CDC source

**CDC Processor Actor**: Consumes CDC stream
- Monitors outbox_messages CDC stream
- Publishes to Redpanda/Kafka
- Implements retry with exponential backoff
- Routes failures to Dead Letter Queue

**Projection Consumers**: Build read models from events
- Independent CDC consumers per projection
- Update denormalized read tables
- Track progress via projection_offsets

### Actor System

**Coordinator Actor**: Supervision root
- Manages child actor lifecycle
- Implements restart policies
- Coordinates graceful shutdown

**Health Monitor Actor**: System health tracking
- Monitors component health
- Aggregates system status
- Provides health check endpoint

**DLQ Actor**: Failed message handling
- Stores failed CDC messages
- Provides retry interface
- Tracks failure statistics

### Messaging Layer

**Redpanda Client**: External event publishing
- Publishes events to Kafka-compatible topics
- Implements circuit breaker pattern
- Connection pooling and management

## Data Flow

### Write Path (Commands)

1. Command arrives at CommandHandler
2. CommandHandler loads aggregate from EventStore
3. Aggregate validates command and emits events
4. EventStore appends events atomically:
   - To event_store table (permanent, source of truth)
   - To outbox_messages table (CDC-enabled, TTL 24h)
5. ScyllaDB CDC detects outbox change
6. CDC stream becomes available to consumers

### Read Path (Projections)

1. CDC Processor consumes from CDC stream
2. Parses event from CDC row
3. Routes to appropriate handlers:
   - Projection consumers → Update read models
   - External publisher → Send to Redpanda
4. Each projection maintains independent offset
5. Failed messages routed to DLQ

### Query Path

Applications query denormalized read models directly:
- order_read_model - Current order state
- orders_by_customer - Orders indexed by customer
- orders_by_status - Orders indexed by status

## Database Schema

### Event Store Tables

**event_store**: Append-only event log
```
Partition Key: aggregate_id
Clustering Key: sequence_number (ascending)
Columns: event_id, event_type, event_version, event_data,
         causation_id, correlation_id, timestamp
```

**aggregate_sequence**: Optimistic locking
```
Primary Key: aggregate_id
Columns: current_sequence, updated_at
```

**outbox_messages**: CDC-enabled outbox
```
Primary Key: id
Columns: aggregate_id, aggregate_type, event_id, event_type,
         event_version, payload, topic, partition_key,
         causation_id, correlation_id, created_at,
         published_at, attempts, last_error
CDC: enabled, postimage, TTL 24h
```

### Projection Tables

**order_read_model**: Current order state
```
Primary Key: order_id
Columns: customer_id, items, status, created_at, updated_at,
         version, is_deleted, deleted_at
```

**orders_by_customer**: Customer's orders
```
Partition Key: customer_id
Clustering Keys: created_at (DESC), order_id (ASC)
Columns: status
```

**orders_by_status**: Orders by status
```
Partition Key: status
Clustering Keys: created_at (DESC), order_id (ASC)
Columns: customer_id
```

### Infrastructure Tables

**dead_letter_queue**: Failed messages
```
Primary Key: id
Columns: aggregate_id, event_type, payload, error_message,
         failure_count, first_failed_at, last_failed_at, created_at
Indexes: event_type, aggregate_id, last_failed_at
```

**projection_offsets**: Projection progress tracking
```
Primary Key: (projection_name, partition_id)
Columns: last_sequence, last_event_id, last_processed_at,
         events_processed, errors_count, last_error
```

## Event Processing Guarantees

### Atomicity

Commands are atomic within a single aggregate:
- Events written to event_store and outbox_messages in single batch
- ScyllaDB ensures atomic batch execution
- Optimistic concurrency prevents conflicting writes

### Consistency

Aggregates enforce business invariants:
- All state changes flow through events
- Events are validated before persistence
- Invalid commands rejected before any writes

### Durability

Events are never deleted or modified:
- event_store is append-only
- Full audit trail maintained
- State can be reconstructed at any point in time

### Idempotency

Event processing is idempotent:
- event_id used for deduplication
- Projections can be safely replayed
- CDC consumers track offsets

## Scalability

### Horizontal Scaling

- Multiple ScyllaDB nodes distribute data
- CDC consumers can run on multiple nodes
- Actor system supports distributed deployment

### Partitioning

- Events partitioned by aggregate_id
- Ensures all events for an aggregate on same partition
- Enables efficient event loading

### Read Scaling

- Read models scaled independently
- Multiple projection tables for different query patterns
- Query optimization per use case

## Monitoring

### Metrics (Prometheus)

- CDC events processed (counter)
- Event store writes (histogram)
- Command processing duration (histogram)
- Circuit breaker state (gauge)
- DLQ message count (gauge)
- Retry attempts (counter)

### Health Checks

- Component health status
- Database connectivity
- Redpanda connectivity
- Actor system status

### Observability

- Structured logging with tracing
- Correlation ID tracking across services
- Causation ID for command-event chains
- Event metadata for debugging

## Error Handling

### Retry Strategy

- Exponential backoff with jitter
- Configurable max attempts
- Per-operation retry configuration

### Circuit Breaker

- Prevents cascading failures
- Automatic recovery after timeout
- State transitions: Closed → Open → Half-Open

### Dead Letter Queue

- Stores permanently failed messages
- Enables manual retry
- Provides failure analysis
- Indexed for efficient querying

## Technology Stack

- **Language**: Rust
- **Database**: ScyllaDB (Cassandra-compatible)
- **Messaging**: Redpanda (Kafka-compatible)
- **Actor Framework**: Actix
- **Metrics**: Prometheus
- **Serialization**: Serde (JSON)
- **Async Runtime**: Tokio

## Design Patterns

- **Event Sourcing**: Events as source of truth
- **CQRS**: Separate write and read models
- **Outbox Pattern**: Reliable event publishing
- **Actor Model**: Fault-tolerant concurrency
- **Circuit Breaker**: Failure isolation
- **Saga Pattern**: (Ready for multi-aggregate transactions)

## Consistency Model

- **Strong consistency** within aggregate (optimistic locking)
- **Eventual consistency** across aggregates
- **Eventual consistency** for read models
- Configurable consistency per read model

## Performance Characteristics

- **Write latency**: ~10-50ms (event store + outbox)
- **CDC latency**: ~50-200ms (ScyllaDB CDC)
- **Projection latency**: ~100-500ms (total write to read)
- **Query latency**: ~1-10ms (direct table reads)

## Future Enhancements

- **Snapshots**: Reduce event replay overhead (schema ready)
- **Event Upcasting**: Schema evolution support (schema ready)
- **Sagas**: Multi-aggregate transactions
- **Event Replay**: Rebuild projections from history
- **Multi-region**: Geographic distribution

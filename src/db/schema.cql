-- ============================================================================
-- Event Sourcing Schema for ScyllaDB with CDC
-- ============================================================================
--
-- Complete Event Sourcing implementation with:
-- 1. Event Store (append-only source of truth)
-- 2. Aggregate management (snapshots, sequences)
-- 3. Projections (read models)
-- 4. Outbox Pattern with CDC enabled
-- 5. Dead Letter Queue for failed messages
--
-- Load this file to set up the complete database:
--   cqlsh -f src/db/schema.cql
--   OR from Docker: docker exec <container> cqlsh -f /schema/schema.cql
--
-- ============================================================================

-- Create keyspace with simple replication (adjust for production)
-- IMPORTANT: Disable tablets to support CDC and secondary indexes
CREATE KEYSPACE IF NOT EXISTS orders_ks
    WITH replication = {'class': 'NetworkTopologyStrategy', 'replication_factor': 1}
    AND tablets = {'enabled': false};

USE orders_ks;

-- ============================================================================
-- LEGACY TABLES - Simple Outbox Pattern (OrderActor)
-- ============================================================================

-- Domain table for orders (used by OrderActor)
CREATE TABLE IF NOT EXISTS orders (
    id          UUID PRIMARY KEY,
    customer_id UUID,
    items       TEXT,           -- JSON-encoded list of OrderItem
    created_at  TIMESTAMP
) WITH comment = 'Legacy order state table for OrderActor demo';


-- ============================================================================
-- EVENT SOURCING TABLES - Full Event Sourcing Implementation
-- ============================================================================

-- Event Store: Append-only source of truth for all aggregates
-- Events are NEVER deleted or modified
CREATE TABLE IF NOT EXISTS event_store (
    -- Partition Key: all events for one aggregate in same partition
    aggregate_id    UUID,

    -- Clustering Key: ensures event ordering
    sequence_number BIGINT,

    -- Event Metadata
    event_id        UUID,           -- Unique identifier for idempotency
    event_type      TEXT,           -- Type of event (e.g., "OrderCreated")
    event_version   INT,            -- Schema version of this event type

    -- Event Payload
    event_data      TEXT,           -- JSON payload of the event

    -- Event Context (for debugging and correlation)
    causation_id    UUID,           -- ID of command/event that caused this event
    correlation_id  UUID,           -- ID linking related events across aggregates

    -- Timestamps
    timestamp       TIMESTAMP,      -- When the event occurred

    PRIMARY KEY (aggregate_id, sequence_number)
) WITH CLUSTERING ORDER BY (sequence_number ASC)
  AND comment = 'Append-only event store - source of truth for all aggregates';

-- Indexes for event queries
CREATE INDEX IF NOT EXISTS idx_event_type ON event_store (event_type);
CREATE INDEX IF NOT EXISTS idx_event_timestamp ON event_store (timestamp);
CREATE INDEX IF NOT EXISTS idx_event_correlation ON event_store (correlation_id);


-- Aggregate Sequence: Tracks current version for optimistic locking
CREATE TABLE IF NOT EXISTS aggregate_sequence (
    aggregate_id        UUID PRIMARY KEY,
    current_sequence    BIGINT,         -- Current sequence number
    updated_at          TIMESTAMP
) WITH comment = 'Current sequence numbers for optimistic concurrency control';


-- Snapshots: Performance optimization (avoid replaying all events)
-- Create snapshot every N events (e.g., every 100)
CREATE TABLE IF NOT EXISTS aggregate_snapshots (
    aggregate_id        UUID,
    sequence_number     BIGINT,         -- Snapshot taken at this event sequence

    -- Snapshot Data
    aggregate_type      TEXT,           -- Type of aggregate (e.g., "Order")
    aggregate_version   INT,            -- Version of aggregate schema
    snapshot_data       TEXT,           -- JSON serialized aggregate state

    -- Snapshot Metadata
    created_at          TIMESTAMP,
    event_count         INT,            -- Total events at time of snapshot

    PRIMARY KEY (aggregate_id, sequence_number)
) WITH CLUSTERING ORDER BY (sequence_number DESC)
  AND comment = 'Snapshots for fast aggregate hydration';


-- ============================================================================
-- OUTBOX PATTERN - Reliable Event Publishing (UNIFIED VERSION)
-- ============================================================================
-- This table is used by BOTH OrderActor and Event Sourcing
-- CDC streams changes from this table for at-least-once delivery
-- Extended schema supports both simple and full event sourcing

CREATE TABLE IF NOT EXISTS outbox_messages (
    id              UUID,           -- Unique outbox entry ID
    aggregate_id    UUID,           -- Reference to aggregate

    -- Event Sourcing fields (may be NULL for legacy OrderActor)
    aggregate_type  TEXT,           -- Type of aggregate (e.g., "Order")
    event_id        UUID,           -- Reference to event in event_store
    event_version   INT,            -- Event schema version

    -- Common fields
    event_type      TEXT,           -- Type of event (e.g., "OrderCreated")
    payload         TEXT,           -- JSON payload to publish

    -- Publishing fields
    topic           TEXT,           -- Target Redpanda/Kafka topic
    partition_key   TEXT,           -- For ordered delivery

    -- Event Context (Event Sourcing)
    causation_id    UUID,           -- Optional: causation tracking
    correlation_id  UUID,           -- Optional: correlation tracking

    -- Timestamps
    created_at      TIMESTAMP,      -- When the event was created
    published_at    TIMESTAMP,      -- When successfully published (NULL = pending)

    -- Status tracking
    attempts        INT,            -- Publish attempt count
    last_error      TEXT,           -- Last error message if failed

    PRIMARY KEY (id)
) WITH cdc = {'enabled': true, 'preimage': false, 'postimage': true, 'ttl': 86400}
  AND default_time_to_live = 86400
  AND comment = 'Transactional outbox for reliable event publishing (supports both legacy and ES)';

-- Index for finding unpublished messages
CREATE INDEX IF NOT EXISTS idx_outbox_published_at ON outbox_messages (published_at);


-- ============================================================================
-- READ MODELS (Projections) - Query Optimization for Event Sourcing
-- ============================================================================
-- Denormalized views built by projecting events from event_store
-- Can be rebuilt at any time by replaying events

-- Current Order State (for queries)
CREATE TABLE IF NOT EXISTS order_read_model (
    order_id        UUID PRIMARY KEY,
    customer_id     UUID,
    items           TEXT,           -- JSON array of order items
    status          TEXT,           -- Current status

    -- Audit Trail
    created_at      TIMESTAMP,
    updated_at      TIMESTAMP,
    version         BIGINT,         -- Current event sequence number

    -- Soft delete support
    is_deleted      BOOLEAN,
    deleted_at      TIMESTAMP
) WITH comment = 'Current order state projection for queries';


-- Orders by Customer (for "my orders" queries)
CREATE TABLE IF NOT EXISTS orders_by_customer (
    customer_id     UUID,
    order_id        UUID,
    created_at      TIMESTAMP,
    status          TEXT,

    PRIMARY KEY (customer_id, created_at, order_id)
) WITH CLUSTERING ORDER BY (created_at DESC, order_id ASC)
  AND comment = 'Orders indexed by customer for fast customer queries';


-- Orders by Status (for operational queries)
CREATE TABLE IF NOT EXISTS orders_by_status (
    status          TEXT,
    created_at      TIMESTAMP,
    order_id        UUID,
    customer_id     UUID,

    PRIMARY KEY (status, created_at, order_id)
) WITH CLUSTERING ORDER BY (created_at DESC, order_id ASC)
  AND comment = 'Orders indexed by status for operational dashboards';


-- ============================================================================
-- PROJECTION TRACKING - Progress and Resumability
-- ============================================================================

-- Projection Offsets: Tracks where each projection is in the event stream
CREATE TABLE IF NOT EXISTS projection_offsets (
    projection_name     TEXT,
    partition_id        INT,

    last_sequence       BIGINT,         -- Last processed sequence
    last_event_id       UUID,           -- Last processed event ID
    last_processed_at   TIMESTAMP,

    -- Projection Health
    events_processed    BIGINT,
    errors_count        INT,
    last_error          TEXT,

    PRIMARY KEY (projection_name, partition_id)
) WITH comment = 'Tracks projection progress for resumability';


-- CDC Offsets: Legacy offset tracking (for backwards compatibility)
-- Note: projection_offsets is preferred for new code
CREATE TABLE IF NOT EXISTS cdc_offsets (
    consumer_id         TEXT,
    table_name          TEXT,
    last_processed_time TIMESTAMP,
    last_event_id       UUID,
    updated_at          TIMESTAMP,
    PRIMARY KEY (consumer_id, table_name)
) WITH comment = 'Legacy CDC offset tracking';


-- ============================================================================
-- DEAD LETTER QUEUE - Failed Message Handling
-- ============================================================================

-- Dead Letter Queue: Stores messages that failed after retries
-- Used by both OrderActor and Event Sourcing
CREATE TABLE IF NOT EXISTS dead_letter_queue (
    id              UUID PRIMARY KEY,

    -- Original Message
    aggregate_id    UUID,
    event_type      TEXT,
    payload         TEXT,

    -- Failure Information
    error_message   TEXT,
    failure_count   INT,
    first_failed_at TIMESTAMP,
    last_failed_at  TIMESTAMP,

    -- Timestamps
    created_at      TIMESTAMP
) WITH comment = 'Dead letter queue for failed messages';

-- Indexes for DLQ queries
CREATE INDEX IF NOT EXISTS dlq_event_type_idx ON dead_letter_queue (event_type);
CREATE INDEX IF NOT EXISTS dlq_aggregate_idx ON dead_letter_queue (aggregate_id);
CREATE INDEX IF NOT EXISTS dlq_failed_at_idx ON dead_letter_queue (last_failed_at);


-- ============================================================================
-- EVENT SCHEMA EVOLUTION - Schema Versioning
-- ============================================================================

-- Event Schemas: Tracks event schema versions for upcasting
-- Allows evolving event schemas while maintaining backward compatibility
CREATE TABLE IF NOT EXISTS event_schemas (
    event_type      TEXT,
    version         INT,

    -- Schema Definition
    schema_json     TEXT,           -- JSON schema definition
    description     TEXT,           -- Human-readable description

    -- Upcasting
    upcast_from     INT,            -- Previous version (for upcasting)
    upcast_script   TEXT,           -- JavaScript/Rust code for upcasting

    -- Metadata
    created_at      TIMESTAMP,
    created_by      TEXT,

    PRIMARY KEY (event_type, version)
) WITH CLUSTERING ORDER BY (version DESC)
  AND comment = 'Event schema versions for evolution and upcasting';


-- ============================================================================
-- USAGE NOTES
-- ============================================================================

-- LEGACY APPROACH (OrderActor):
-- 1. Write to orders table
-- 2. Write to outbox_messages (basic fields: id, aggregate_id, event_type, payload, created_at)
-- 3. CDC streams outbox_messages to Redpanda
-- 4. DLQ stores failed messages

-- EVENT SOURCING APPROACH (CommandHandler):
-- 1. Validate command against aggregate
-- 2. Generate events
-- 3. Write to event_store + outbox_messages (atomic, extended fields)
-- 4. Update aggregate_sequence (optimistic locking)
-- 5. CDC streams outbox_messages to:
--    a. Redpanda (external systems)
--    b. Projections (read models)
-- 6. DLQ stores failed messages
-- 7. Snapshots created every 100 events

-- PROJECTION APPROACH (Direct CDC):
-- 1. Projection CDC consumer reads from outbox_messages CDC stream
-- 2. Updates read models (order_read_model, orders_by_customer, etc.)
-- 3. Tracks progress in projection_offsets
-- 4. Can rebuild by replaying events from event_store

-- COMPATIBILITY:
-- - Both OrderActor and EventStore can write to outbox_messages
-- - OrderActor uses basic fields (id, aggregate_id, event_type, payload, created_at)
-- - EventStore uses extended fields (adds event_id, event_version, causation_id, etc.)
-- - CDC processor reads both formats
-- - Fields not populated by OrderActor will be NULL

-- ============================================================================
-- END OF SCHEMA
-- ============================================================================

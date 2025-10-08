-- Keyspace
CREATE KEYSPACE IF NOT EXISTS orders_ks
    WITH replication = {'class': 'NetworkTopologyStrategy', 'replication_factor': 1};

USE orders_ks;

-- Domain table
CREATE TABLE IF NOT EXISTS orders
(
    id          UUID PRIMARY KEY,
    customer_id UUID,
    items       TEXT, -- JSON-encoded list of OrderItem
    created_at  TIMESTAMP
) WITH default_time_to_live = 0;

-- Outbox table WITH CDC ENABLED
-- This table stores all domain events that need to be published
-- CDC is enabled to stream changes to external systems (Redpanda/Kafka)
CREATE TABLE IF NOT EXISTS outbox_messages
(
    id           UUID,           -- Unique event ID
    aggregate_id UUID,           -- The ID of the aggregate (e.g., order_id)
    event_type   TEXT,           -- Type of event (e.g., "OrderCreated")
    payload      TEXT,           -- JSON payload of the event
    created_at   TIMESTAMP,      -- When the event was created
    PRIMARY KEY (id)
) WITH cdc = {'enabled': true, 'preimage': false, 'postimage': true, 'ttl': 86400};

-- CDC offset tracking table
-- Stores the last processed position from the CDC stream
CREATE TABLE IF NOT EXISTS cdc_offsets
(
    consumer_id      TEXT,       -- Identifier for the CDC consumer
    table_name       TEXT,       -- Name of the table being tracked
    last_processed_time TIMESTAMP,  -- Last event timestamp processed
    last_event_id    UUID,       -- Last event ID processed (for idempotency)
    updated_at       TIMESTAMP,  -- When this offset was last updated
    PRIMARY KEY (consumer_id, table_name)
);
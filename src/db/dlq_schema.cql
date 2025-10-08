-- Dead Letter Queue Schema
-- Stores failed messages that could not be published after retries

USE orders_ks;

-- Dead Letter Queue table
-- Stores messages that failed after all retry attempts
CREATE TABLE IF NOT EXISTS dead_letter_queue
(
    id              UUID,           -- Original event ID
    aggregate_id    UUID,           -- The aggregate this event belongs to
    event_type      TEXT,           -- Type of event (e.g., "OrderCreated")
    payload         TEXT,           -- JSON payload of the event
    error_message   TEXT,           -- Error that caused the failure
    failure_count   INT,            -- Number of times this message failed
    first_failed_at TIMESTAMP,      -- When the message first failed
    last_failed_at  TIMESTAMP,      -- Most recent failure time
    created_at      TIMESTAMP,      -- When added to DLQ
    PRIMARY KEY (id)
);

-- Index for querying by event type
CREATE INDEX IF NOT EXISTS dlq_event_type_idx ON dead_letter_queue (event_type);

-- Index for querying by aggregate
CREATE INDEX IF NOT EXISTS dlq_aggregate_idx ON dead_letter_queue (aggregate_id);

-- Index for querying by failure time
CREATE INDEX IF NOT EXISTS dlq_failed_at_idx ON dead_letter_queue (last_failed_at);

# Schema vs Query Audit - Mismatches Found

## Critical Mismatches Found

### ❌ **MISMATCH 1: event_store table - Missing columns**

**Schema** (schema.cql lines 45-69):
```sql
CREATE TABLE IF NOT EXISTS event_store (
    aggregate_id    UUID,
    sequence_number BIGINT,
    event_id        UUID,
    event_type      TEXT,
    event_version   INT,
    event_data      TEXT,
    causation_id    UUID,
    correlation_id  UUID,
    timestamp       TIMESTAMP,
    PRIMARY KEY (aggregate_id, sequence_number)
);
```

**Query in event_store.rs** (line 72-75):
```rust
"INSERT INTO event_store (
    aggregate_id, sequence_number, event_id, event_type, event_version,
    event_data, causation_id, correlation_id, user_id, timestamp, metadata
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
```

**PROBLEM**: Query tries to insert into columns that **DON'T EXIST**:
- ❌ `user_id` - NOT in schema
- ❌ `metadata` - NOT in schema

**Impact**: This query will fail at runtime with "Unknown identifier" error.

---

### ❌ **MISMATCH 2: aggregate_sequence table - Wrong UPDATE syntax**

**Schema** (schema.cql lines 77-82):
```sql
CREATE TABLE IF NOT EXISTS aggregate_sequence (
    aggregate_id        UUID PRIMARY KEY,
    current_sequence    BIGINT,
    updated_at          TIMESTAMP
);
```

**Query in event_store.rs** (line 92):
```rust
"UPDATE aggregate_sequence SET current_sequence = ?, updated_at = ? WHERE aggregate_id = ?"
```

**PROBLEM**: Using UPDATE on a table where the row might not exist yet.

**Issue**: For new aggregates, UPDATE won't create the row. Should use INSERT or UPSERT behavior.

**Better approach**:
```sql
INSERT INTO aggregate_sequence (aggregate_id, current_sequence, updated_at) VALUES (?, ?, ?)
```
(ScyllaDB INSERT has upsert semantics)

---

### ❌ **MISMATCH 3: event_store SELECT - Missing columns**

**Query in event_store.rs** (line 160-164):
```rust
"SELECT aggregate_id, sequence_number, event_id, event_type, event_version,
        event_data, causation_id, correlation_id, timestamp
 FROM event_store
 WHERE aggregate_id = ?
 ORDER BY sequence_number ASC"
```

**Schema has these columns**: ✅ All columns exist

**Code expects** (line 178):
```rust
rows::<(Uuid, i64, Uuid, String, i32, String, Option<Uuid>, Uuid, chrono::DateTime<Utc>)>()
```

**Column mapping**:
1. aggregate_id - Uuid ✅
2. sequence_number - i64 ✅
3. event_id - Uuid ✅
4. event_type - String ✅
5. event_version - i32 ✅
6. event_data - String ✅
7. causation_id - Option<Uuid> ✅
8. correlation_id - Uuid ✅
9. timestamp - DateTime<Utc> ✅

This one is **OK** ✅

---

### ⚠️ **MISMATCH 4: Batch statement issues**

**Code in event_store.rs** (lines 57-144):

The batch preparation has structural issues:

1. **Loop runs twice** (lines 61-88 and 98-138):
   - First loop: Appends statements to batch
   - Second loop: Creates values

2. **new_version incremented in both loops**:
   ```rust
   let mut new_version = expected_version;

   for event_envelope in &events {
       new_version += 1;  // First increment (line 62)
       // ...
   }

   for event_envelope in &events {
       new_version += 1;  // Second increment (line 99) - WRONG!
       // ...
   }
   ```

**PROBLEM**: `new_version` gets incremented TWICE per event!

**Impact**: Version numbers will be wrong, causing concurrency control to fail.

---

## Additional Schema Tables

### orders_by_customer table

**Schema** (schema.cql lines 175-183):
```sql
CREATE TABLE IF NOT EXISTS orders_by_customer (
    customer_id     UUID,
    order_id        UUID,
    created_at      TIMESTAMP,
    status          TEXT,
    PRIMARY KEY (customer_id, created_at, order_id)
);
```

**Query in projections.rs** (line 223-225):
```rust
"INSERT INTO orders_by_customer (
    customer_id, order_id, created_at, status, total_amount
) VALUES (?, ?, ?, ?, ?)"
```

**PROBLEM**: Query tries to insert `total_amount` column that **DOESN'T EXIST** in schema.

❌ **Missing column**: `total_amount`

---

### projection_offsets table

**Schema** (schema.cql lines 203-217):
```sql
CREATE TABLE IF NOT EXISTS projection_offsets (
    projection_name     TEXT,
    partition_id        INT,
    last_sequence       BIGINT,
    last_event_id       UUID,
    last_processed_at   TIMESTAMP,
    -- Additional fields:
    events_processed    BIGINT,
    errors_count        INT,
    last_error          TEXT,
    PRIMARY KEY (projection_name, partition_id)
);
```

**Query in projections.rs** (line 180-185):
```rust
"INSERT INTO projection_offsets (
    projection_name, partition_id, last_sequence,
    last_event_id, last_processed_at
) VALUES (?, 0, ?, ?, ?)"
```

This one is **OK** ✅ (Optional columns can be NULL)

---

## Summary of Issues

| Issue | File | Line | Severity | Description |
|-------|------|------|----------|-------------|
| 1 | event_store.rs | 72-75 | 🔴 CRITICAL | INSERT into non-existent columns `user_id`, `metadata` |
| 2 | event_store.rs | 92 | 🟡 WARNING | UPDATE won't work for new aggregates, use INSERT |
| 3 | event_store.rs | 61-138 | 🔴 CRITICAL | Double version increment in batch loop |
| 4 | projections.rs | 223-225 | 🔴 CRITICAL | INSERT into non-existent column `total_amount` |

---

## Recommended Fixes

### Fix 1: Remove user_id and metadata from event_store INSERT

**Option A**: Remove from query (simplest)
```rust
"INSERT INTO event_store (
    aggregate_id, sequence_number, event_id, event_type, event_version,
    event_data, causation_id, correlation_id, timestamp
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
```

**Option B**: Add columns to schema
```sql
ALTER TABLE event_store ADD user_id UUID;
ALTER TABLE event_store ADD metadata TEXT;  -- or MAP<TEXT, TEXT>
```

### Fix 2: Change UPDATE to INSERT for aggregate_sequence

```rust
"INSERT INTO aggregate_sequence (aggregate_id, current_sequence, updated_at)
 VALUES (?, ?, ?)"
```

### Fix 3: Fix double increment in batch loop

Remove the second loop or fix the version tracking:
```rust
let mut new_version = expected_version;

// Build batch statements AND values in ONE loop
for event_envelope in &events {
    new_version += 1;

    // Serialize once
    let event_json = serialize_event(&event_envelope.event_data)?;

    // Add statement
    batch.append_statement("INSERT INTO event_store ...");

    // Add values immediately (don't wait for second loop)
    values.push(Box::new((...)));

    if publish_to_outbox {
        batch.append_statement("INSERT INTO outbox_messages ...");
        values.push(Box::new((...)));
    }
}

// Add aggregate_sequence update ONCE
batch.append_statement("INSERT INTO aggregate_sequence ...");
values.push(Box::new((aggregate_id, new_version, Utc::now())));
```

### Fix 4: Remove total_amount from orders_by_customer INSERT

**Option A**: Remove from query
```rust
"INSERT INTO orders_by_customer (
    customer_id, order_id, created_at, status
) VALUES (?, ?, ?, ?)"
```

**Option B**: Add column to schema
```sql
ALTER TABLE orders_by_customer ADD total_amount DECIMAL;
```

---

## Schema Columns vs Code Usage

### event_store table
| Column | Schema | INSERT Query | SELECT Query |
|--------|--------|--------------|--------------|
| aggregate_id | ✅ | ✅ | ✅ |
| sequence_number | ✅ | ✅ | ✅ |
| event_id | ✅ | ✅ | ✅ |
| event_type | ✅ | ✅ | ✅ |
| event_version | ✅ | ✅ | ✅ |
| event_data | ✅ | ✅ | ✅ |
| causation_id | ✅ | ✅ | ✅ |
| correlation_id | ✅ | ✅ | ✅ |
| timestamp | ✅ | ✅ | ✅ |
| user_id | ❌ | ❌ USED | ❌ |
| metadata | ❌ | ❌ USED | ❌ |

### outbox_messages table
| Column | Schema | Query |
|--------|--------|-------|
| id | ✅ | ✅ |
| aggregate_id | ✅ | ✅ |
| aggregate_type | ✅ | ✅ |
| event_id | ✅ | ✅ |
| event_type | ✅ | ✅ |
| event_version | ✅ | ✅ |
| payload | ✅ | ✅ |
| topic | ✅ | ✅ |
| partition_key | ✅ | ✅ |
| causation_id | ✅ | ✅ |
| correlation_id | ✅ | ✅ |
| created_at | ✅ | ✅ |
| attempts | ✅ | ✅ |
| published_at | ✅ | ❌ not used |
| last_error | ✅ | ❌ not used |

### orders_by_customer table
| Column | Schema | Query |
|--------|--------|-------|
| customer_id | ✅ | ✅ |
| order_id | ✅ | ✅ |
| created_at | ✅ | ✅ |
| status | ✅ | ✅ |
| total_amount | ❌ | ❌ USED |

---

## Next Steps

1. **Fix event_store.rs** - Remove user_id, metadata from INSERT
2. **Fix event_store.rs** - Change UPDATE to INSERT for aggregate_sequence
3. **Fix event_store.rs** - Fix double version increment bug
4. **Fix projections.rs** - Remove total_amount from INSERT
5. **Test queries** - Verify all queries work with actual database

These fixes are **REQUIRED** before the application can run successfully.

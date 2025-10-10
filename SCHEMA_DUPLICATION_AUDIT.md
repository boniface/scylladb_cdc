# Schema Duplication Audit

## Problem: Schema Created in Multiple Places

The schema is created in **THREE different places**, causing confusion and potential inconsistencies:

### 1. ✅ **schema.cql** (Primary Source of Truth)
**Location**: `src/db/schema.cql`

**What it creates**:
- Complete keyspace definition: `orders_ks` with `NetworkTopologyStrategy` and tablets disabled
- All Event Sourcing tables (event_store, aggregate_sequence, snapshots, etc.)
- All projection tables (order_read_model, orders_by_customer, orders_by_status)
- Outbox pattern tables (outbox_messages with CDC enabled)
- Dead letter queue tables
- All indexes

**Used by**:
- ✅ Makefile `schema` target: Loads via Docker
- ✅ `fix_schema.sh`: Loads via Docker
- ✅ `init_db.sh`: Loads via cqlsh

### 2. ⚠️ **main.rs** (Partial Duplication)
**Location**: `src/main.rs` lines 40-47

**What it creates**:
```rust
session.query_unpaged(
    "CREATE KEYSPACE IF NOT EXISTS orders_ks WITH REPLICATION = \
     {'class': 'SimpleStrategy', 'replication_factor': 1} \
     AND tablets = {'enabled': false}",
    &[],
).await?;
```

**ONLY creates**:
- ❌ Keyspace `orders_ks` with `SimpleStrategy` (different from schema.cql!)
- ❌ Does NOT create any tables
- ❌ Does NOT create indexes
- ❌ Does NOT enable CDC

**Issues**:
1. **Replication strategy mismatch**:
   - schema.cql: `NetworkTopologyStrategy`
   - main.rs: `SimpleStrategy`
2. **Incomplete**: Relies on schema.cql being loaded separately
3. **Confusing**: Looks like it sets up the database, but doesn't

### 3. 🗑️ **init_db.sh** (Redundant)
**Location**: `scripts/init_db.sh`

**What it does**:
- Waits for ScyllaDB to be ready
- Executes `src/db/schema.cql` via cqlsh
- Verifies tables were created

**Issues**:
- ❌ NOT used by Makefile
- ❌ NOT used by docker-compose
- ❌ Redundant with `make schema` command
- ❌ Adds confusion

---

## Comparison: Who Creates What?

| What | schema.cql | main.rs | init_db.sh | Makefile |
|------|------------|---------|------------|----------|
| Keyspace | ✅ NetworkTopologyStrategy | ⚠️ SimpleStrategy | ✅ (via schema.cql) | ✅ (via schema.cql) |
| event_store | ✅ | ❌ | ✅ (via schema.cql) | ✅ (via schema.cql) |
| aggregate_sequence | ✅ | ❌ | ✅ (via schema.cql) | ✅ (via schema.cql) |
| outbox_messages | ✅ with CDC | ❌ | ✅ (via schema.cql) | ✅ (via schema.cql) |
| Projections | ✅ | ❌ | ✅ (via schema.cql) | ✅ (via schema.cql) |
| Indexes | ✅ | ❌ | ✅ (via schema.cql) | ✅ (via schema.cql) |

---

## Redundant Scripts

### ❌ **verify_schema.sh** (Outdated)
**Location**: `verify_schema.sh` (root)

**Purpose**: Checks if aggregate_type column exists

**Issues**:
- Only checks ONE column
- Provides manual fix instructions
- Redundant with `make schema`
- Confusing for users

### ❌ **fix_schema.sh** (Redundant)
**Location**: `fix_schema.sh` (root)

**Purpose**: Clean restart + load schema

**Issues**:
- Does EXACTLY what `make reset` does
- Duplicate functionality
- Adds confusion

---

## Current Workflow (Confusing)

```
User wants to set up DB
    ↓
Which one to use?
    ├─ make reset        ✅ Works
    ├─ make schema       ✅ Works
    ├─ init_db.sh        ⚠️ Works but redundant
    ├─ fix_schema.sh     ⚠️ Works but redundant
    └─ verify_schema.sh  ⚠️ Only verifies
```

Then main.rs runs and creates keyspace AGAIN (with different settings!) ⚠️

---

## Recommended Cleanup

### ✅ KEEP: schema.cql (Single Source of Truth)
**Why**: Complete schema definition, used by Docker and Makefile

**No changes needed**

### ✅ KEEP: Makefile targets
**Why**: Standard interface for all operations

Commands to keep:
- `make reset` - Clean restart + load schema
- `make schema` - Load schema into running DB
- `make dev` - Start services + load schema + run app

### ❌ REMOVE: main.rs keyspace creation
**Why**:
1. Schema is already loaded by Makefile before app runs
2. Creates replication strategy mismatch
3. Incomplete (doesn't create tables)
4. Confusing

**File**: `src/main.rs` lines 39-47

### ❌ REMOVE: init_db.sh
**Why**: Completely redundant with `make schema`

**File**: `scripts/init_db.sh`

### ❌ REMOVE: fix_schema.sh
**Why**: Completely redundant with `make reset`

**File**: `fix_schema.sh`

### ❌ REMOVE: verify_schema.sh
**Why**: Obsolete, only checks one column

**File**: `verify_schema.sh`

---

## Proposed Clean Workflow

```
User wants to set up DB
    ↓
make reset           (first time or after errors)
    ↓
    1. docker-compose down -v
    2. docker-compose up -d
    3. sleep 25
    4. Load schema.cql
    ↓
make run             (runs the app)
    ↓
    App connects to DB
    App does NOT create keyspace
    All tables already exist ✅
```

**Benefits**:
- ✅ Single source of truth (schema.cql)
- ✅ No duplication
- ✅ No replication strategy mismatch
- ✅ Clear workflow
- ✅ Less confusion

---

## Implementation Plan

### Step 1: Remove keyspace creation from main.rs

**File**: `src/main.rs`

**Remove lines 39-49**:
```rust
// REMOVE THIS:
session.query_unpaged(
    "CREATE KEYSPACE IF NOT EXISTS orders_ks WITH REPLICATION = \
     {'class': 'SimpleStrategy', 'replication_factor': 1} \
     AND tablets = {'enabled': false}",
    &[],
).await?;

session.use_keyspace("orders_ks", false).await?;
```

**Replace with**:
```rust
// Use existing keyspace (created by schema.cql)
session.use_keyspace("orders_ks", false).await?;
```

**Why**: Schema is already loaded by `make reset` or `make schema` before the app runs.

### Step 2: Delete redundant scripts

```bash
rm scripts/init_db.sh
rm fix_schema.sh
rm verify_schema.sh
```

If `scripts/` directory is now empty, remove it:
```bash
rmdir scripts/
```

### Step 3: Update README.md (if needed)

Ensure README only mentions:
- `make reset` - First time setup
- `make run` - Run application
- `make schema` - Reload schema (if needed)

Remove any references to:
- init_db.sh
- fix_schema.sh
- verify_schema.sh

---

## Verification After Cleanup

After cleanup, there should be **ONE WAY** to create the schema:

```bash
# First time or after errors
make reset

# This runs (internally):
# 1. docker-compose down -v
# 2. docker-compose up -d
# 3. sleep 25
# 4. docker exec $(docker-compose ps -q scylla) cqlsh -f /schema/schema.cql
```

Then run the app:
```bash
make run

# App connects and uses existing keyspace
# No schema creation in app code
```

**Single Source of Truth**: `src/db/schema.cql` ✅

---

## Summary

### Current State (Confusing)
- 📄 schema.cql - Creates everything
- 🔧 main.rs - Creates keyspace (different settings!)
- 📜 init_db.sh - Loads schema.cql (redundant)
- 📜 fix_schema.sh - Loads schema.cql (redundant)
- 📜 verify_schema.sh - Checks schema (partial)

**Result**: 5 places dealing with schema, inconsistencies, confusion ❌

### After Cleanup (Clear)
- 📄 schema.cql - Creates everything ✅
- 🔧 main.rs - Uses existing keyspace only ✅
- 🛠️ Makefile - Loads schema.cql ✅

**Result**: Single source of truth, clear workflow ✅

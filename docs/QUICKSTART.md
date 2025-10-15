# Quick Start Guide

## ⚠️ IMPORTANT: First Time or After Errors

If you're running for the **first time** or got the error:
```
Error: Unknown identifier aggregate_type
```

You MUST do a **clean restart**:

```bash
make reset
make run
```

This will:
1. Remove old containers and volumes (`docker-compose down -v`)
2. Start fresh containers
3. Wait for ScyllaDB to be ready (25 seconds)
4. Load the complete Event Sourcing schema
5. You can then run the app

## Run the Application

### First Time Setup

```bash
# Clean reset (removes old schema)
make reset

# Run application
make run
```

### Subsequent Runs

If containers are already running and schema is loaded:

```bash
make run
```

Or start everything from scratch:

```bash
make dev
```

## What to Expect

```
- Starting ScyllaDB Event Sourcing with CDC
- Event Sourcing + CQRS + Direct CDC Projections

════════════════════════════════════════════════════════════
 Event Sourcing Demo - Full Order Lifecycle
════════════════════════════════════════════════════════════

1️⃣  Creating order via Event Sourcing CommandHandler...
   ✅ Order created: <uuid> (version: 1)
   📦 Events written to event_store table
   📤 Events written to outbox_messages table (atomic)
   🌊 CDC will stream to projections and Redpanda

2️⃣  Confirming order...
   ✅ Order confirmed (version: 2)

3️⃣  Shipping order...
   ✅ Order shipped (version: 3)
   📦 Tracking: TRACK-123-XYZ (DHL Express)

4️⃣  Delivering order...
   ✅ Order delivered (version: 4)
   ✍️  Signed by: John Doe

5️⃣  Aggregate verification: ✅ EXISTS

🎉 Event Sourcing Demo Complete!
```

## 🔍 Verify Schema is Correct

```bash
# Check if aggregate_type column exists
docker exec $(docker-compose ps -q scylla) cqlsh -e "USE orders_ks; DESC TABLE outbox_messages;" | grep aggregate_type

# Should show: aggregate_type  TEXT,
```

## 🐛 Troubleshooting

### Error: "Unknown identifier aggregate_type"

**Cause**: Old table schema exists without Event Sourcing columns

**Fix**:
```bash
make reset
make run
```

### Error: "unconfigured table aggregate_sequence"

**Cause**: Schema not loaded

**Fix**:
```bash
make reset
make run
```

### Error: "Connection refused"

**Cause**: ScyllaDB not ready

**Fix**: Wait longer (ScyllaDB takes ~20-25 seconds)
```bash
docker-compose up -d
sleep 25
make schema
cargo run
```

### Verify Everything

```bash
# 1. Check containers running
docker-compose ps

# 2. Check schema loaded
docker exec $(docker-compose ps -q scylla) cqlsh -e "USE orders_ks; DESC TABLES;"

# Should show:
# - event_store
# - aggregate_sequence
# - outbox_messages (with aggregate_type, event_id, etc.)
# - order_read_model
# - orders_by_customer
# - orders_by_status
# - dead_letter_queue
# - and more...
```

## 🧹 Clean Up

```bash
# Stop and remove everything
make clean
```

## Available Commands

```bash
make help       # Show all commands
make reset      # Clean restart (recommended first time)
make run        # Run application
make dev        # Start services + run app
make test       # Run tests
make build      # Build release
make schema     # (Re)load schema
make metrics    # View Prometheus metrics
make clean      # Stop and clean up
```

## Quick Commands Reference

```bash
# First time or after errors
make reset && make run

# Normal run (if everything is set up)
make run

# Start from scratch
make dev

# Clean everything
make clean
```

## Why "make reset"?

ScyllaDB's `CREATE TABLE IF NOT EXISTS` doesn't alter existing tables. If an old schema exists with fewer columns, it won't be updated. 

`make reset` does `docker-compose down -v` which:
- Stops containers
- **Removes volumes** (deletes old schema)
- Ensures fresh start with correct schema

## More Information

- **README.md** - Complete documentation
- **EVENT_SOURCING_GUIDE.md** - Event Sourcing concepts
- **CDC_PROJECTIONS_ARCHITECTURE.md** - CDC architecture

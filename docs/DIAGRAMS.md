# Visual understanding of the distributed systems issues and  the problem this small project is trying to address using  ScyllaDB + Rust + Actix ecosystem.



## Table of Contents

1. [The Dual-Write Problem](#the-dual-write-problem)
2. [Outbox Pattern Solution](#outbox-pattern-solution)
3. [CDC Architecture](#cdc-architecture)
4. [Retry Flow Diagram](#retry-flow-diagram)
5. [Actor Supervision Tree](#actor-supervision-tree)
6. [Circuit Breaker State Machine](#circuit-breaker-state-machine)
7. [Complete Data Flow](#complete-data-flow)
8. [Failure Scenarios](#failure-scenarios)

---

## The Dual-Write Problem

### Naive Approach (Problematic)

```
┌──────────────┐
│   Service    │
│              │
└──────┬───────┘
       │
       │ create_order(order)
       ▼
┌─────────────────────────────────────────┐
│                                         │
│  1. Save to Database                    │
│     ├─► INSERT INTO orders ...          │
│     └─► ✅ SUCCESS                      │
│                                         │
│  2. Publish to Message Queue            │
│     ├─► publish(OrderCreated)           │
│     └─► ❌ NETWORK TIMEOUT              │
│                                         │
└─────────────────────────────────────────┘

Result:
┌──────────────────┐     ┌──────────────────┐
│   Database       │     │  Message Queue   │
│                  │     │                  │
│  ✅ Order saved  │     │  ❌ No event     │
│                  │     │                  │
└──────────────────┘     └──────────────────┘

PROBLEM: Inconsistent state!
- Order exists in database
- Downstream services never notified
- Inventory not updated
- Customer not emailed
```


### Timing Diagram: Failure Scenarios

```
Timeline: Naive Dual-Write Approach

Scenario 1: Database fails
────────────────────────────────────────────
Service     │ DB Write    Publish
────────────┼──────────┼───────────
T0: Start   │          │
T1: Write   │ ❌ FAIL  │ (not reached)
T2: End     │          │
Result: Consistent (no data, no event) ✅

Scenario 2: Message Queue fails
────────────────────────────────────────────
Service     │ DB Write    Publish
────────────┼──────────-┼───────────
T0: Start   │           │
T1: Write   │ ✅ SUCCESS│
T2: Publish │           │ ❌ FAIL
T3: End     │           │
Result: INCONSISTENT ❌
  DB: Order exists
  MQ: No event

Scenario 3: Network timeout
────────────────────────────────────────────
Service     │ DB Write    Publish
────────────┼──────────-┼───────────
T0: Start   │           │
T1: Write   │ ✅ SUCCESS│
T2: Publish │           │ ⏳ TIMEOUT
T3: Retry?  │           │ ??? UNKNOWN STATE
Result: INCONSISTENT ❌
  DB: Order exists
  MQ: Maybe published (we don't know!)
```

---

## Outbox Pattern Solution

### Correct Approach

```
┌──────────────┐
│   Service    │
│              │
└──────┬───────┘
       │
       │ create_order(order)
       ▼
┌─────────────────────────────────────────────┐
│        Transactional Batch Write            │
│                                             │
│  BEGIN BATCH;                               │
│    INSERT INTO orders ...;                  │
│    INSERT INTO outbox_messages ...;         │
│  END BATCH;                                 │
│                                             │
│  Result: ✅ Both succeed or both fail       │
└─────────────────────────────────────────────┘
       │
       ▼
┌──────────────────────────────────────────────┐
│          ScyllaDB (Single Database)          │
│                                              │
│  ┌────────────────┐  ┌──────────────────┐    │
│  │  orders        │  │ outbox_messages  │    │
│  │                │  │                  │    │
│  │  id: UUID      │  │  id: UUID        │    │
│  │  customer_id   │  │  aggregate_id ───┼───-┤
│  │  items         │  │  event_type      │    │
│  │  status        │  │  payload (JSON)  │    │
│  └────────────────┘  └──────────────────┘    │
│                                              │
└──────────────────────────────────────────────┘
       │
       │ CDC Stream (automatic)
       ▼
┌──────────────────────-┐
│  CDC Stream Processor │
│  (Separate Process)   │
└──────────┬────────────┘
           │
           │ Publish events
           ▼
┌──────────────────────┐
│   Redpanda/Kafka     │
│   (Message Queue)    │
└──────────────────────┘
```

### Timing Diagram: Outbox Pattern

```
Timeline: Transactional Outbox Pattern

Scenario 1: Happy path
─────────────────────────────────────────────────────
Service         │ Batch Write     CDC Processor    Publish
────────────────┼──────────────┼──────────────┼──────────
T0: Start       │              │              │
T1: Batch write │ ✅ SUCCESS   │              │
T2: CDC detects │              │ ✅ Received  │
T3: Publish     │              │              │ ✅ SUCCESS
Result: Consistent ✅

Scenario 2: Database fails
─────────────────────────────────────────────────────
Service         │ Batch Write     CDC Processor    Publish
────────────────┼──────────────┼──────────────┼──────────
T0: Start       │              │              │
T1: Batch write │ ❌ FAIL      │ (no event)   │ (not reached)
T2: End         │              │              │
Result: Consistent (no data, no event) ✅

Scenario 3: Message Queue fails
─────────────────────────────────────────────────────
Service         │ Batch Write     CDC Processor    Publish
────────────────┼──────────────┼──────────────┼──────────
T0: Start       │              │              │
T1: Batch write │ ✅ SUCCESS   │              │
T2: CDC detects │              │ ✅ Received  │
T3: Publish     │              │              │ ❌ FAIL
T4: Retry 1     │              │              │ ❌ FAIL
T5: Retry 2     │              │              │ ❌ FAIL
T6: DLQ store   │              │ ✅ DLQ saved │
T7: Later       │              │ Manual retry │ ✅ SUCCESS
Result: Eventually consistent ✅
  DB: Order + outbox message exist
  DLQ: Event stored for manual handling
  Later: Event successfully published
```

---

## CDC Architecture

### How CDC Works in ScyllaDB

```
┌─────────────────────────────────────────────────────────-┐
│                    Application Layer                     │ 
│                                                          │
│  ┌─────────────┐                                         │
│  │ OrderActor  │                                         │
│  └──────┬──────┘                                         │
│         │                                                │
│         │ Batch INSERT                                   │
│         ▼                                                │
└─────────┼────────────────────────────────────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────-┐
│                   ScyllaDB Layer                         │
│                                                          │
│  ┌──────────────────────────────────────────────┐        │
│  │         Base Table: outbox_messages          │        │
│  │  ┌──────────────────────────────────────┐    │        │
│  │  │ INSERT INTO outbox_messages ...      │    │        │
│  │  └──────────────────────────────────────┘    │        │
│  └──────────────────────────────────────────────┘        │
│          │                                               │
│          │ (CDC enabled)                                 │
│          │ Automatic replication                         │
│          ▼                                               │
│  ┌──────────────────────────────────────────────┐        │
│  │      Hidden CDC Log Tables (per stream)      │        │
│  │                                              │        │
│  │  Stream 0: [row1, row2, row3, ...]           │        │
│  │  Stream 1: [row4, row5, row6, ...]           │        │
│  │  Stream 2: [row7, row8, row9, ...]           │        │
│  │  ...                                         │        │
│  │                                              │        │
│  │  Each stream = subset of partition tokens    │        │
│  └──────────────────────────────────────────────┘        │
│          │                                               │
│          │ Streams organized by VNode                    │
│          ▼                                               │
└─────────┼────────────────────────────────────────────────┘
          │
          │ CDC Library reads streams
          ▼
┌─────────────────────────────────────────────────────────┐
│              CDC Stream Processor Layer                 │
│                                                         │
│  ┌─────────────────────────────────────────────┐        │
│  │       CDCLogReader (from scylla-cdc)        │        │
│  │                                             │        │
│  │  • Discovers CDC streams                    │        │
│  │  • Creates consumers per stream             │        │
│  │  • Handles generation changes               │        │
│  │  • Manages checkpointing                    │        │
│  └────────┬────────┬────────┬────────┬─────────┘        │
│           │        │        │        │                  │
│           ▼        ▼        ▼        ▼                  │
│     ┌─────────┐ ┌─────────┐ ┌─────────┐                 │
│     │Consumer │ │Consumer │ │Consumer │ ... (parallel)  │
│     │ Stream0 │ │ Stream1 │ │ Stream2 │                 │
│     └────┬────┘ └────┬────┘ └────┬────┘                 │
│          │           │           │                      │
│          │ consume_cdc()         │                      │
│          ▼           ▼           ▼                      │
│     ┌──────────────────────────────────┐                │
│     │  OutboxCDCConsumer               │                │
│     │  • Extract event data            │                │
│     │  • Retry with backoff            │                │
│     │  • Send to DLQ on failure        │                │
│     └──────────────┬───────────────────┘                │
│                    │                                    │
└────────────────────┼────────────────────────────────────┘
                     │
                     │ Publish events
                     ▼
            ┌─────────────────┐
            │ Redpanda/Kafka  │
            └─────────────────┘
```

### CDC Stream Distribution

```
Partition Token Range: [0 ... 2^64-1]

┌──────────────────────────────────────────────────────┐
│              ScyllaDB Cluster                        │
│                                                      │
│  Node 1                 Node 2                Node 3 │
│  ┌──────────┐          ┌──────────┐         ┌──────────-┐
│  │ Tokens:  │          │ Tokens:  │         │ Tokens:   │
│  │ 0 - 33%  │          │ 33% - 66%│         │ 66% - 100%│
│  └──────────┘          └──────────┘         └──────────-┘
│      │                     │                     │
│      │ CDC enabled         │ CDC enabled         │ CDC enabled
│      ▼                     ▼                     ▼
│  ┌──────────┐          ┌──────────┐         ┌──────────┐
│  │ Stream 0 │          │ Stream 1 │         │ Stream 2 │
│  │ Stream 3 │          │ Stream 4 │         │ Stream 5 │
│  │   ...    │          │   ...    │         │   ...    │
│  └──────────┘          └──────────┘         └──────────┘
└──────────────────────────────────────────────────────┘
       │                     │                     │
       └─────────┬───────────┴─────────┬───────────┘
                 │                     │
                 ▼                     ▼
       ┌──────────────────┐  ┌──────────────────┐
       │ Consumer Factory │  │ Consumer Factory │
       │   (Instance 1)   │  │   (Instance 2)   │
       │                  │  │                  │
       │ • Reads subset   │  │ • Reads subset   │
       │   of streams     │  │   of streams     │
       │ • Parallel       │  │ • Parallel       │
       │   processing     │  │   processing     │
       └──────────────────┘  └──────────────────┘

Key Benefits:
• Parallel processing across streams
• Horizontal scalability
• Fault isolation
• Load balancing
```

---

## Retry Flow Diagram

### Retry with Exponential Backoff

```
┌────────────────────────────────────────────────────────┐
│                   Retry State Machine                  │
└────────────────────────────────────────────────────────┘

State: ATTEMPT_1
─────────────────
Time: T0
Delay: 0ms (immediate)
│
├─► Execute operation()
│
├─► Result?
│   │
│   ├─► ✅ Success
│   │   └─► Return RetryResult::Success
│   │
│   └─► ❌ Failure
│       │
│       ├─► is_transient()?
│       │   │
│       │   ├─► No (permanent)
│       │   │   └─► Return RetryResult::PermanentFailure
│       │   │
│       │   └─► Yes (transient)
│       │       │
│       │       └─► Sleep 100ms
│                   │
│                   ▼
State: ATTEMPT_2
─────────────────
Time: T0 + 100ms
Delay: 100ms
│
├─► Execute operation()
│
├─► Result?
│   │
│   ├─► ✅ Success
│   │   └─► Return RetryResult::Success
│   │
│   └─► ❌ Failure (transient)
│       │
│       └─► Sleep 200ms (100ms * 2.0)
│           │
│           ▼
State: ATTEMPT_3
─────────────────
Time: T0 + 300ms
Delay: 200ms
│
├─► Execute operation()
│
├─► Result?
│   │
│   ├─► ✅ Success
│   │   └─► Return RetryResult::Success
│   │
│   └─► ❌ Failure (transient)
│       │
│       └─► Sleep 400ms (200ms * 2.0)
│           │
│           ▼
State: ATTEMPT_4
─────────────────
Time: T0 + 700ms
Delay: 400ms
│
├─► Execute operation()
│
├─► Result?
│   │
│   ├─► ✅ Success
│   │   └─► Return RetryResult::Success
│   │
│   └─► ❌ Failure (transient)
│       │
│       └─► Sleep 500ms (min(400ms * 2.0, 500ms) = 500ms)
│           │
│           ▼
State: ATTEMPT_5 (FINAL)
─────────────────
Time: T0 + 1200ms
Delay: 500ms (capped)
│
├─► Execute operation()
│
├─► Result?
│   │
│   ├─► ✅ Success
│   │   └─► Return RetryResult::Success
│   │
│   └─► ❌ Failure (transient)
│       │
│       └─► Max attempts reached
│           │
│           └─► Return RetryResult::Failed
│               │
│               ▼
            ┌─────────┐
            │   DLQ   │
            └─────────┘

Total Time: 1.2 seconds
Total Attempts: 5
Backoff Sequence: 0ms, 100ms, 200ms, 400ms, 500ms
```

### Retry Decision Tree

```
                    ┌─────────────┐
                    │  Operation  │
                    │   Execute   │
                    └──────┬──────┘
                           │
                           ▼
                    ┌──────────────┐
                    │   Success?   │
                    └──────┬───────┘
                           │
              ┌────────────┴────────────┐
              │                         │
              ▼                         ▼
         ┌─────────┐              ┌─────────┐
         │   YES   │              │   NO    │
         └────┬────┘              └────┬────┘
              │                        │
              ▼                        ▼
      ┌──────────────┐        ┌───────────────┐
      │ Return       │        │  Is Error     │
      │ Success(T)   │        │  Transient?   │
      └──────────────┘        └───────┬───────┘
                                      │
                         ┌────────────┴────────────┐
                         │                         │
                         ▼                         ▼
                    ┌─────────┐              ┌─────────┐
                    │   YES   │              │   NO    │
                    └────┬────┘              └────┬────┘
                         │                        │
                         ▼                        ▼
                ┌────────────────┐      ┌──────────────────┐
                │  Attempts <    │      │ Return Permanent │
                │  Max?          │      │ Failure(E)       │
                └────┬───────────┘      └──────────────────┘
                     │
        ┌────────────┴────────────┐
        │                         │
        ▼                         ▼
   ┌─────────┐              ┌─────────┐
   │   YES   │              │   NO    │
   └────┬────┘              └────┬────┘
        │                        │
        ▼                        ▼
   ┌──────────────┐      ┌──────────────┐
   │ Sleep with   │      │ Return       │
   │ Backoff      │      │ Failed(E)    │
   └──────┬───────┘      └──────────────┘
          │                     │
          ▼                     ▼
   ┌──────────────┐      ┌──────────────┐
   │ Retry        │      │   Send to    │
   │ (loop back)  │      │     DLQ      │
   └──────────────┘      └──────────────┘
```

---

## Actor Supervision Tree

```
                   ┌────────────────────────────────┐
                   │     Actix System Root          │
                   │  (Manages entire actor system) │
                   └───────────────┬────────────────┘
                                   │
                                   │ spawns
                                   ▼
                   ┌────────────────────────────────┐
                   │    CoordinatorActor            │
                   │  (Supervisor)                  │
                   │                                │
                   │  Responsibilities:             │
                   │  • Manages child lifecycles    │
                   │  • Handles graceful shutdown   │
                   │  • Coordinates health checks   │
                   │  • Implements restart policies │
                   └───────────────┬────────────────┘
                                   │
                                   │ supervises
                ┌──────────────────┼──────────────────┐
                │                  │                  │
                ▼                  ▼                  ▼
     ┌─────────────────┐  ┌──────────────┐  ┌──────────────┐
     │ HealthCheckActor│  │  DlqActor    │  │  OrderActor  │
     │                 │  │              │  │              │
     │ • Monitors CB   │  │ • Stores     │  │ • Processes  │
     │ • Checks health │  │   failed     │  │   commands   │
     │ • Reports       │  │   messages   │  │ • Transact   │
     │   status        │  │ • Provides   │  │   writes     │
     │                 │  │   stats      │  │              │
     └─────────────────┘  └──────────────┘  └──────┬───────┘
                                                   │
                                                   │ writes to
                                                   ▼
                    ┌────────────────────────────────────────┐
                    │         ScyllaDB                       │
                    │  ┌────────────┐  ┌──────────────────┐  │
                    │  │  orders    │  │ outbox_messages  │  │
                    │  └────────────┘  └────────┬─────────┘  │
                    └───────────────────────────┼────────────┘
                                                │
                                                │ CDC stream
                                                ▼
                    ┌────────────────────────────────────────┐
                    │    CdcProcessor                        │
                    │                                        │
                    │  • Consumes CDC events                 │
                    │  • Retries with backoff                │
                    │  • Sends to DLQ on failure             │
                    │                                        │
                    │  Dependencies:                         │
                    │  ├─► RedpandaClient (with CB)          │
                    │  └─► DlqActor (for failures)           │
                    └─────────────────┬──────────────────────┘
                                      │
                                      │ publishes to
                                      ▼
                    ┌────────────────────────────────────────┐
                    │         Redpanda/Kafka                 │
                    │                                        │
                    │  • OrderCreated topic                  │
                    │  • OrderUpdated topic                  │
                    │  • OrderCancelled topic                │
                    └────────────────────────────────────────┘

Supervision Policies:
═══════════════════════════════════════════════════════════
Actor                Policy               Action on Failure
─────────────────────────────────────────────────────────
CoordinatorActor     N/A (root)          System shutdown
HealthCheckActor     Resume              Log error, continue
DlqActor             Resume              Log error, continue
OrderActor           Restart             Recreate actor
CdcProcessor   Restart             Recreate consumer

Message Flow:
═══════════════════════════════════════════════════════════
1. User → OrderActor (CreateOrder message)
2. OrderActor → ScyllaDB (batched write)
3. ScyllaDB → CdcProcessor (CDC stream)
4. CdcProcessor → RedpandaClient (publish)
5. On failure → DlqActor (store failed message)
6. HealthCheckActor → All actors (periodic health checks)
```

---

## Circuit Breaker State Machine

```
                  ┌──────────────┐
                  │    CLOSED    │
                  │   (Normal)   │
                  │              │
                  │ • All calls  │
                  │   allowed    │
                  │ • Track      │
                  │   failures   │
                  └──────┬───────┘
                         │
                         │ failure_count++
                         │
     ┌───────────────────┴─────────────────┐
     │                                     │
     │ failure_count >= threshold (5)?     │
     │                                     │
     └────────┬────────────────────────────┘
              │
              │ YES
              ▼
       ┌──────────────┐
       │     OPEN     │
       │  (Blocking)  │
       │              │
       │ • All calls  │
       │   rejected   │
       │ • Fast fail  │
       │ • Start      │
       │   timeout    │
       └──────┬───────┘
              │
              │ wait timeout (30s)
              │
              ▼
       ┌──────────────┐
       │   HALF-OPEN  │
       │   (Testing)  │
       │              │
       │ • Limited    │
       │   calls      │
       │ • Test       │
       │   recovery   │
       └──────┬───────┘
              │
              │ execute test calls
              │
     ┌────────┴─────────────────────────┐
     │                                  │
     ▼                                  ▼
┌─────────┐                        ┌─────────┐
│ SUCCESS │                        │ FAILURE │
│   ?     │                        │   ?     │
└────┬────┘                        └────┬────┘
     │                                  │
     │ success_count >= 3?              │
     │                                  │
     ▼                                  ▼
┌─────────┐                        ┌─────────┐
│   YES   │                        │   NO    │
└────┬────┘                        └────┬────┘
     │                                  │
     │ Reset failure_count              │
     ▼                                  ▼
┌──────────────┐              ┌──────────────┐
│ Transition   │              │ Transition   │
│ to CLOSED    │              │ to OPEN      │
└──────────────┘              └──────────────┘
     │                                  │
     └──────────────┬───────────────────┘
                    │
                    ▼
            Resume operations

State Transition Table:
═══════════════════════════════════════════════════════════
From State  │ Event              │ To State    │ Action
────────────┼────────────────────┼─────────────┼──────────
CLOSED      │ failure_count >= 5 │ OPEN        │ Block calls
CLOSED      │ success            │ CLOSED      │ Reset counter
OPEN        │ timeout elapsed    │ HALF_OPEN   │ Allow test
OPEN        │ call attempted     │ OPEN        │ Reject fast
HALF_OPEN   │ success (count=3)  │ CLOSED      │ Resume
HALF_OPEN   │ any failure        │ OPEN        │ Block again

Timing Example:
═══════════════════════════════════════════════════════════
T0:    CLOSED - call succeeds (failure_count = 0)
T1:    CLOSED - call fails (failure_count = 1)
T2:    CLOSED - call fails (failure_count = 2)
T3:    CLOSED - call fails (failure_count = 3)
T4:    CLOSED - call fails (failure_count = 4)
T5:    CLOSED - call fails (failure_count = 5)
T6:    → OPEN (threshold reached)
T7-35: OPEN - all calls rejected immediately
T36:   → HALF_OPEN (30s timeout elapsed)
T37:   HALF_OPEN - test call succeeds (success_count = 1)
T38:   HALF_OPEN - test call succeeds (success_count = 2)
T39:   HALF_OPEN - test call succeeds (success_count = 3)
T40:   → CLOSED (recovery confirmed)
```

---

## Complete Data Flow

### End-to-End: Order Creation to Event Delivery

```
Step 1: User Request
════════════════════════════════════════════════════════════
┌──────────┐
│   User   │ POST /orders {"customer_id": "...", "items": [...]}
└─────┬────┘
      │
      ▼
┌──────────────────┐
│  HTTP Handler    │
│  (Not shown in   │
│   this project)  │
└─────┬────────────┘
      │
      │ Send CreateOrder message
      ▼

Step 2: Actor Processing
════════════════════════════════════════════════════════════
┌──────────────────────────────────────────────────────┐
│              OrderActor::handle()                    │
│                                                      │
│  1. Generate order_id = UUID::new_v4()              │
│  2. Create OrderCreatedEvent                        │
│  3. Call persist_with_outbox()                      │
│     ├─► Prepare batch                               │
│     ├─► Add order INSERT                            │
│     ├─► Add outbox INSERT                           │
│     └─► Execute atomically                          │
│                                                      │
└──────────────────┬───────────────────────────────────┘
                   │
                   ▼

Step 3: Database Write
════════════════════════════════════════════════════════════
┌──────────────────────────────────────────────────────┐
│                   ScyllaDB                           │
│                                                      │
│  BEGIN BATCH;                                        │
│    INSERT INTO orders                                │
│      (id, customer_id, items, status, ...)           │
│    VALUES (?, ?, ?, 'pending', ...);                 │
│                                                      │
│    INSERT INTO outbox_messages                       │
│      (id, aggregate_id, event_type, payload, ...)    │
│    VALUES (?, ?, 'OrderCreated', '{"order_id":...}', │
│             ...);                                    │
│  APPLY BATCH;                                        │
│                                                      │
│  ✅ Both writes succeed atomically                   │
│                                                      │
└──────────────────┬───────────────────────────────────┘
                   │
                   │ Automatically triggers CDC
                   ▼

Step 4: CDC Capture (< 100ms latency)
════════════════════════════════════════════════════════════
┌──────────────────────────────────────────────────────┐
│            CDC Hidden Log Tables                     │
│                                                      │
│  Stream 0: [..., new_row, ...]                      │
│  Stream 1: [...]                                     │
│  Stream 2: [...]                                     │
│                                                      │
│  new_row = {                                         │
│    operation: "RowInsert",                           │
│    stream_id: 0,                                     │
│    data: {                                           │
│      id: UUID,                                       │
│      aggregate_id: UUID,                             │
│      event_type: "OrderCreated",                     │
│      payload: '{"order_id": "..."}',                 │
│      created_at: TIMESTAMP                           │
│    }                                                 │
│  }                                                   │
│                                                      │
└──────────────────┬───────────────────────────────────┘
                   │
                   │ CDC library polls streams
                   ▼

Step 5: Consumer Processing
════════════════════════════════════════════════════════════
┌──────────────────────────────────────────────────────┐
│     OutboxCDCConsumer::consume_cdc()                 │
│                                                      │
│  1. Receive CDCRow                                   │
│  2. Extract event data                               │
│     ├─► id                                           │
│     ├─► aggregate_id                                 │
│     ├─► event_type                                   │
│     └─► payload                                      │
│                                                      │
│  3. Attempt publish with retry                       │
│     ├─► Attempt 1: immediate                         │
│     ├─► Attempt 2: after 100ms (if failed)           │
│     ├─► Attempt 3: after 200ms (if failed)           │
│     ├─► Attempt 4: after 400ms (if failed)           │
│     └─► Attempt 5: after 500ms (if failed)           │
│                                                      │
└──────────────────┬───────────────────────────────────┘
                   │
       ┌───────────┴───────────┐
       │                       │
       ▼                       ▼
  ┌─────────┐            ┌─────────┐
  │ Success │            │ Failure │
  └────┬────┘            └────┬────┘
       │                      │
       ▼                      ▼

Step 6a: Success Path
════════════════════════════════════════════════════════════
┌──────────────────────────────────────────────────────┐
│         RedpandaClient::publish()                    │
│                                                      │
│  Through circuit breaker:                            │
│  1. Check circuit state                              │
│  2. If CLOSED, send to Redpanda                      │
│  3. Update success metrics                           │
│                                                      │
└──────────────────┬───────────────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────────────────────┐
│              Redpanda/Kafka                          │
│                                                      │
│  Topic: OrderCreated                                 │
│  Partition: hash(order_id) % partitions              │
│  Message: {                                          │
│    key: "order_id",                                  │
│    value: '{"order_id": "...", "customer_id": ...}'  │
│  }                                                   │
│                                                      │
│  ✅ Event successfully published                     │
│                                                      │
└──────────────────┬───────────────────────────────────┘
                   │
                   ▼
        ┌────────────────────┐
        │  Downstream        │
        │  Consumers         │
        │  • Inventory Svc   │
        │  • Email Svc       │
        │  • Analytics       │
        └────────────────────┘

Step 6b: Failure Path
════════════════════════════════════════════════════════════
┌──────────────────────────────────────────────────────┐
│         After 5 failed retry attempts                │
│                                                      │
│  DlqActor::handle(AddToDlq)                         │
│  1. Log error with context                           │
│  2. INSERT INTO dead_letter_queue                    │
│     ├─► Original event data                          │
│     ├─► Error message                                │
│     ├─► Failure count (5)                            │
│     ├─► First failed timestamp                       │
│     └─► Last failed timestamp                        │
│                                                      │
│  3. Update DLQ metrics                               │
│                                                      │
└──────────────────┬───────────────────────────────────┘
                   │
                   ▼
        ┌────────────────────┐
        │  Alert System      │
        │  • PagerDuty       │
        │  • Slack           │
        │  • Email           │
        └────────┬───────────┘
                 │
                 ▼
        ┌────────────────────┐
        │  On-Call Engineer  │
        │  • Investigate DLQ │
        │  • Fix root cause  │
        │  • Replay messages │
        └────────────────────┘

Latency Breakdown (Happy Path):
════════════════════════════════════════════════════════════
Operation                      Latency
─────────────────────────────────────────────────────────
1. User request → Actor        ~1ms
2. Actor processing            ~0.5ms
3. ScyllaDB batch write        ~2-5ms (local)
4. CDC capture                 ~50-100ms
5. Consumer processing         ~1ms
6. Redpanda publish            ~2-5ms (local)
─────────────────────────────────────────────────────────
Total end-to-end:              ~60-120ms

With retries (worst case):     ~1.2s + base latency
```

---

## Failure Scenarios

### Scenario Matrix

```
┌──────────────────────────────────────────────────────────────────┐
│            Failure Scenarios and System Response                 │
└──────────────────────────────────────────────────────────────────┘

Scenario 1: Redpanda Temporary Outage
═══════════════════════════════════════════════════════════════════
Timeline:
─────────
T0: Order created, batch written to ScyllaDB ✅
T1: CDC captures change ✅
T2: Attempt 1: Redpanda connection refused ❌
T3: Wait 100ms
T4: Attempt 2: Still down ❌
T5: Wait 200ms
T6: Attempt 3: Still down ❌
T7: Wait 400ms
T8: Attempt 4: Redpanda recovers! ✅
Result: Event successfully published after 700ms

Impact:
───────
• Order saved: ✅
• Event eventually published: ✅
• User notified: ✅ (order confirmed)
• Downstreams notified: ✅ (delayed ~700ms)

Scenario 2: Redpanda Extended Outage
═══════════════════════════════════════════════════════════════════
Timeline:
─────────
T0: Order created, batch written ✅
T1: CDC captures change ✅
T2-T6: All 5 retry attempts fail ❌
T7: Message sent to DLQ ✅
T8: Alert triggered 🚨
T9: Engineer investigates
T10: Redpanda fixed
T11: Engineer replays DLQ messages ✅

Impact:
───────
• Order saved: ✅
• Event in DLQ: ✅
• User notified: ✅ (order confirmed)
• Downstreams notified: ❌ (waiting on manual replay)
• SLA: Degraded, but no data loss

Scenario 3: ScyllaDB Write Failure
═══════════════════════════════════════════════════════════════════
Timeline:
─────────
T0: Attempt batch write
T1: ScyllaDB returns error (timeout/unavailable) ❌
T2: OrderActor receives error
T3: Returns error to user ❌

Impact:
───────
• Order saved: ❌
• Event published: ❌
• User notified: ❌ (received error response)
• System state: Consistent (nothing saved) ✅

Scenario 4: Partial CDC Processing Failure
═══════════════════════════════════════════════════════════════════
Timeline:
─────────
T0: 100 orders created
T1: CDC captures all 100 changes ✅
T2: Consumer processes 50 events successfully ✅
T3: Redpanda overloaded, circuit breaker opens 🔴
T4: Next 50 events rejected by circuit breaker ❌
T5: All 50 failed events sent to DLQ ✅
T6: Circuit breaker timeout (30s)
T7: Circuit breaker enters HALF_OPEN 🟡
T8: 3 test events succeed ✅
T9: Circuit breaker closes 🟢
T10: DLQ messages replayed ✅

Impact:
───────
• Orders saved: ✅ (all 100)
• Events published: ✅ (50 immediate + 50 after recovery)
• Circuit breaker prevented cascading failure ✅
• Total delay for last 50: ~30s

Scenario 5: Consumer Crash
═══════════════════════════════════════════════════════════════════
Timeline:
─────────
T0: Consumer processing events normally
T1: OOM/panic, consumer crashes ☠️
T2: Actix supervisor detects failure
T3: Supervisor restarts CdcProcessor ♻️
T4: CDC library resumes from last checkpoint ✅
T5: Events re-processed (idempotency handles duplicates) ✅

Impact:
───────
• Orders saved: ✅
• Events eventually published: ✅
• Possible duplicates: Handled by idempotency ✅
• Downtime: ~1-2 seconds

Recovery Decision Tree:
═══════════════════════════════════════════════════════════════════
                     ┌────────────┐
                     │  Failure   │
                     │  Detected  │
                     └──────┬─────┘
                            │
                   ┌────────┴─────────┐
                   │                  │
                   ▼                  ▼
            ┌──────────────┐   ┌──────────────┐
            │  Transient   │   │  Permanent   │
            │    Error?    │   │    Error?    │
            └──────┬───────┘   └──────┬───────┘
                   │                  │
                   ▼                  ▼
            ┌──────────────┐   ┌──────────────┐
            │    Retry     │   │    Send to   │
            │ with Backoff │   │     DLQ      │
            └──────┬───────┘   └──────┬───────┘
                   │                  │
      ┌────────────┴────────┐         │
      │                     │         │
      ▼                     ▼         ▼
┌──────────┐         ┌──────────┐  ┌─────────────┐
│ Success  │         │ Exhaust  │  │   Alert     │
│ After N  │         │ Retries  │  │  On-Call    │
│ Attempts │         └─────┬────┘  └─────────────┘
└──────────┘               │
                           ▼
                    ┌──────────────┐
                    │   Send to    │
                    │     DLQ      │
                    └──────────────┘
```

---




## Documentation Links

- [Return to Documentation Index](INDEX.md) - Back to the main documentation index
- [Return to README](../README.md) - Back to main project page
- [Main Tutorial](TUTORIAL.md) - Complete Event Sourcing tutorial with rich diagrams

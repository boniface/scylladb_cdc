# Visual understanding of the distributed systems issues and the problem this small project is trying to address using ScyllaDB + Rust + Actix ecosystem.

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

```mermaid
graph TD
    subgraph "Service Layer"
        A[Service]
    end
    
    subgraph "Step 1: Database Write"
        B[Save to Database<br/>INSERT INTO orders]
        B_SUCCESS[SUCCESS]
        B_FAIL[FAIL]
    end
    
    subgraph "Step 2: Message Queue Publish"
        C[Publish to Message Queue<br/>publish OrderCreated]
        C_SUCCESS[SUCCESS]
        C_FAIL[NETWORK TIMEOUT]
    end
    
    subgraph "Results"
        D[Database: Order Saved]
        E[Message Queue: No Event]
    end

    A --> B
    B --> B_SUCCESS
    B --> B_FAIL
    B_SUCCESS --> C
    C --> C_SUCCESS
    C --> C_FAIL
    B_SUCCESS --> D
    C_FAIL --> E
    
    style A fill:#ffcdd2,stroke:#b71c1c,stroke-width:2px
    style B_FAIL fill:#ffcdd2,stroke:#b71c1c,stroke-width:2px
    style C_FAIL fill:#ffcdd2,stroke:#b71c1c,stroke-width:2px
    style D fill:#c8e6c9,stroke:#388e3c,stroke-width:2px
    style E fill:#ffcdd2,stroke:#b71c1c,stroke-width:2px
```

**PROBLEM**: Inconsistent state!
- Order exists in database
- Downstream services never notified
- Inventory not updated
- Customer not emailed

### Timing Diagram: Failure Scenarios

```mermaid
graph LR
    subgraph "Scenario 1: Database Fails"
        S1_START[Start] --> S1_DB_FAIL["DB Write: ❌ FAIL"]
        S1_DB_FAIL --> S1_END["End: Consistent ✅"]
    end
    
    subgraph "Scenario 2: Message Queue Fails"  
        S2_START[Start] --> S2_DB_SUCCESS["DB Write: ✅ SUCCESS"]
        S2_DB_SUCCESS --> S2_MQ_FAIL["MQ Publish: ❌ FAIL"]
        S2_MQ_FAIL --> S2_RESULT["INCONSISTENT ❌<br/>DB: Order exists<br/>MQ: No event"]
    end
    
    subgraph "Scenario 3: Network Timeout"
        S3_START[Start] --> S3_DB_SUCCESS["DB Write: ✅ SUCCESS"] 
        S3_DB_SUCCESS --> S3_TIMEOUT["MQ Publish: ⏳ TIMEOUT"]
        S3_TIMEOUT --> S3_UNKNOWN["UNKNOWN STATE<br/>DB: Order exists<br/>MQ: Maybe published"]
    end
```

---

## Outbox Pattern Solution

### Correct Approach

```mermaid
graph TD
    subgraph "Application Layer"
        A[Service]
    end
    
    subgraph "Transactional Batch Write"
        B[BEGIN BATCH<br/>INSERT INTO orders<br/>INSERT INTO outbox_messages<br/>END BATCH]
        B_RESULT[✅ Both succeed or both fail]
    end
    
    subgraph "ScyllaDB Layer"
        C[ScyllaDB<br/>├─ orders table<br/>└─ outbox_messages table<br/>with CDC enabled]
    end
    
    subgraph "CDC Processing"
        D[CDC Stream Processor]
        E[Redpanda/Kafka]
    end

    A --> B
    B --> B_RESULT
    B_RESULT --> C
    C --> D
    D --> E
    
    style A fill:#c8e6c9,stroke:#388e3c,stroke-width:2px
    style B_RESULT fill:#c8e6c9,stroke:#388e3c,stroke-width:2px
    style C fill:#c8e6c9,stroke:#388e3c,stroke-width:2px
```

### Timing Diagram: Outbox Pattern

```mermaid
graph LR
    subgraph "Scenario 1: Happy Path"
        SP1_START[Start] --> SP1_BATCH[Batch Write SUCCESS]
        SP1_BATCH --> SP1_CDC[CDP Detects SUCCESS]
        SP1_CDC --> SP1_PUB[Publish SUCCESS]
        SP1_PUB --> SP1_RESULT[Consistent SUCCESS]
    end
    
    subgraph "Scenario 2: Database Fails"
        SP2_START[Start] --> SP2_BATCH_FAIL[Batch Write FAIL]
        SP2_BATCH_FAIL --> SP2_NOEVENT[No event, No DB - Consistent SUCCESS]
    end
    
    subgraph "Scenario 3: Message Queue Fails"
        SP3_START[Start] --> SP3_BATCH[Batch Write SUCCESS]
        SP3_BATCH --> SP3_CDC[CDP Detects SUCCESS]
        SP3_CDC --> SP3_PUB_FAIL[Pub FAIL]
        SP3_PUB_FAIL --> SP3_RETRY[Retry to DLQ]
        SP3_RETRY --> SP3_RESULT["Eventually consistent<br/>DB: Order exists<br/>DLQ: Event for handling"]
    end
```

---

## CDC Architecture

### How CDC Works in ScyllaDB

```mermaid
graph TB
    subgraph "Application Layer"
        A[OrderCommandHandler<br/>Batch INSERT to ScyllaDB]
    end
    
    subgraph "ScyllaDB Layer"
        B[Base Table: outbox_messages]
        C[CDC Hidden Log Tables]
    end
    
    subgraph "CDC Streams"
        D[Stream 0: row1 row2 ...]
        E[Stream 1: row4 row5 ...]
        F[Stream 2: row7 row8 ...]
    end
    
    subgraph "CDC Processing Layer"
        G[CDCLogReader]
        H[Consumer Stream0]
        I[Consumer Stream1] 
        J[Consumer Stream2]
        K[OutboxCDCConsumer]
    end
    
    subgraph "Event Publishing"
        L[Redpanda/Kafka]
    end

    A --> B
    B --> C
    C --> D
    C --> E
    C --> F
    G --> H
    G --> I
    G --> J
    H --> K
    I --> K
    J --> K
    K --> L
    
    style A fill:#e3f2fd,stroke:#0d47a1,stroke-width:2px
    style B fill:#e8f5e8,stroke:#2e7d32,stroke-width:2px
    style C fill:#e8f5e8,stroke:#2e7d32,stroke-width:2px
    style G fill:#fff3e0,stroke:#e65100,stroke-width:2px
    style K fill:#f3e5f5,stroke:#7b1fa2,stroke-width:2px
    style L fill:#ffccbc,stroke:#bf360c,stroke-width:2px
```

### CDC Stream Distribution

```mermaid
graph LR
    subgraph "ScyllaDB Cluster"
        A[Node 1<br/>Tokens: 0-33%]
        B[Node 2<br/>Tokens: 33%-66%]
        C[Node 3<br/>Tokens: 66%-100%]
    end
    
    subgraph "CDC Streams per Node"
        A --> A_STREAMS[Stream 0, Stream 3, ...]
        B --> B_STREAMS[Stream 1, Stream 4, ...]
        C --> C_STREAMS[Stream 2, Stream 5, ...]
    end
    
    subgraph "Consumer Distribution"
        A_STREAMS --> FACTORY1[Consumer Factory 1]
        B_STREAMS --> FACTORY1
        C_STREAMS --> FACTORY2[Consumer Factory 2]
    end
    
    subgraph "Benefits"
        BENEFITS["• Parallel processing<br/>• Horizontal scalability<br/>• Fault isolation<br/>• Load balancing"]
    end

    style A fill:#e3f2fd,stroke:#0d47a1,stroke-width:2px
    style B fill:#e3f2fd,stroke:#0d47a1,stroke-width:2px
    style C fill:#e3f2fd,stroke:#0d47a1,stroke-width:2px
    style FACTORY1 fill:#fff3e0,stroke:#e65100,stroke-width:2px
    style FACTORY2 fill:#fff3e0,stroke:#e65100,stroke-width:2px
```

---

## Retry Flow Diagram

### Retry with Exponential Backoff

```mermaid
stateDiagram-v2
    [*] --> ATTEMPT_1 : Execute operation
    ATTEMPT_1 --> SUCCESS : Success
    ATTEMPT_1 --> FAILURE_1 : Failure (transient)
    
    FAILURE_1 --> SLEEP_100MS : Sleep 100ms
    SLEEP_100MS --> ATTEMPT_2 : Next attempt
    ATTEMPT_2 --> SUCCESS : Success
    ATTEMPT_2 --> FAILURE_2 : Failure (transient)
    
    FAILURE_2 --> SLEEP_200MS : Sleep 200ms
    SLEEP_200MS --> ATTEMPT_3 : Next attempt
    ATTEMPT_3 --> SUCCESS : Success
    ATTEMPT_3 --> FAILURE_3 : Failure (transient)
    
    FAILURE_3 --> SLEEP_400MS : Sleep 400ms
    SLEEP_400MS --> ATTEMPT_4 : Next attempt
    ATTEMPT_4 --> SUCCESS : Success
    ATTEMPT_4 --> FAILURE_4 : Failure (transient)
    
    FAILURE_4 --> SLEEP_500MS : Sleep 500ms (capped)
    SLEEP_500MS --> ATTEMPT_5 : Final attempt
    ATTEMPT_5 --> SUCCESS : Success
    ATTEMPT_5 --> MAX_REACHED : Failure (transient)
    
    MAX_REACHED --> SEND_TO_DLQ : Max attempts reached
    SEND_TO_DLQ --> [*]
```

**State Notes:**
- **SUCCESS**: Return RetryResult::Success, Total time: varies, Attempts: 1-N
- **SEND_TO_DLQ**: Return RetryResult::Failed, Send to DLQ for manual processing, Total time: ~1.2 seconds, Backoff: 0,100,200,400,500ms

### Retry Decision Tree

```mermaid
flowchart TD
    A[Execute Operation] --> B{Success?}
    B -->|Yes| C[Return Success]
    B -->|No| D{Is Error Transient?}
    D -->|No| E[Return Permanent Failure]
    D -->|Yes| F{Attempts < Max?}
    F -->|Yes| G[Sleep with Backoff]
    G --> H[Retry Operation]
    H --> B
    F -->|No| I[Send to DLQ]
    E --> J[Handle Error]
    I --> K[Manual Processing]
    
    style A fill:#c8e6c9
    style C fill:#c8e6c9
    style E fill:#ffcdd2
    style I fill:#fff3e0
    style K fill:#fff3e0
```

---

## Actor Supervision Tree

The system uses actors only for infrastructure components, not for domain logic:

```mermaid
graph TD
    subgraph "Actor System Root"
        ROOT[System Root<br/>Manages entire actor system]
    end
    
    subgraph "Supervisor Layer"
        COORD[CoordinatorActor<br/>Supervisor]
        COORD --> HC[HealthMonitorActor<br/>Health checks]
        COORD --> DLQ[DLQActor<br/>Failed messages]
        COORD --> CDC_PROC[CDCProcessor Actor<br/>CDC event streaming]
    end
    
    subgraph "Data Layer"
        SCYLLA[ScyllaDB<br/>event_store + outbox_messages]
        SCYLLA --> CDC_PROC
    end
    
    subgraph "External Systems"  
        CDC_PROC --> REDPANDA[Redpanda/Kafka<br/>Event publishing]
    end

    ROOT --> COORD
    
    style ROOT fill:#e3f2fd,stroke:#0d47a1,stroke-width:2px
    style COORD fill:#e1f5fe,stroke:#0277bd,stroke-width:2px
    style HC fill:#fff3e0,stroke:#e65100,stroke-width:2px
    style DLQ fill:#ffebee,stroke:#b71c1c,stroke-width:2px
    style CDC_PROC fill:#e8f5e8,stroke:#2e7d32,stroke-width:2px
    style SCYLLA fill:#f3e5f5,stroke:#7b1fa2,stroke-width:2px
    style REDPANDA fill:#e0f2f1,stroke:#00695c,stroke-width:2px
```

**Supervision Policies:**
```
Actor                Policy               Action on Failure
─────────────────────────────────────────────────────────
CoordinatorActor     N/A (root)          System shutdown
HealthMonitorActor   Resume              Log error, continue
DlqActor             Resume              Log error, continue
CdcProcessor         Restart             Recreate consumer
```

**Message Flow:**
```
1. Command Handler → ScyllaDB (batched write - event_store + outbox)
2. ScyllaDB → CdcProcessor (CDC stream)
3. CdcProcessor → RedpandaClient (publish with retry)
4. On failure → DlqActor (store failed message)
5. HealthMonitorActor → All actors (periodic health checks)
```

**Important**: Domain logic (Order Aggregate, OrderCommandHandler) are pure Rust components, NOT actors.

---

## Circuit Breaker State Machine

```mermaid
stateDiagram-v2
    [*] --> CLOSED
    CLOSED --> OPEN: Failure Count >= 5
    OPEN --> HALF_OPEN: Timeout Elapsed (30s)
    HALF_OPEN --> CLOSED: Success Threshold Met (3)
    HALF_OPEN --> OPEN: Failure Occurred
    CLOSED --> [*]: Success
    OPEN --> [*]: Fast Fail
    HALF_OPEN --> [*]: Recovery Test
```

**State Details:**
- **CLOSED**: 
  - Normal operation
  - All calls allowed
  - Track failures
  - Threshold typically 5 failures
- **OPEN**:
  - Timeout typically 30 seconds
- **HALF_OPEN**:
  - Typically 3 successful calls needed

**State Transition Table:**
```
From State  │ Event              │ To State    │ Action
────────────┼────────────────────┼─────────────┼──────────
CLOSED      │ failure_count >= 5 │ OPEN        │ Block calls
CLOSED      │ success            │ CLOSED      │ Reset counter
OPEN        │ timeout elapsed    │ HALF_OPEN   │ Allow test
OPEN        │ call attempted     │ OPEN        │ Reject fast
HALF_OPEN   │ success (count=3)  │ CLOSED      │ Resume
HALF_OPEN   │ any failure        │ OPEN        │ Block again
```

**Timing Example:**
```
T0:    CLOSED - call succeeds (failure_count = 0)
T1:    CLOSED - call fails (failure_count = 1)  
T2:    CLOSED - call fails (failure_count = 2)
T3:    CLOSED - call fails (failure_count = 3)
T4:    CLOSED - call fails (failure_count = 4)
T5:    CLOSED - call fails (failure_count = 5)
T6:    -> OPEN (threshold reached)
T7-35: OPEN - all calls rejected immediately
T36:   -> HALF_OPEN (30s timeout elapsed)
T37:   HALF_OPEN - test call succeeds (success_count = 1)
T38:   HALF_OPEN - test call succeeds (success_count = 2)
T39:   HALF_OPEN - test call succeeds (success_count = 3)
T40:   -> CLOSED (recovery confirmed)
```

---

## Complete Data Flow

### End-to-End: Order Creation to Event Delivery

```mermaid
sequenceDiagram
    autonumber
    participant User
    participant API
    participant CommandHandler
    participant ScyllaDB
    participant CDCProcessor
    participant Redpanda
    participant Downstream
    
    User->>API: POST /orders {"customer_id": "...", "items": [...]}
    API->>CommandHandler: CreateOrder Command
    activate CommandHandler
    CommandHandler->>ScyllaDB: Atomic Batch Write<br/>(event_store + outbox_messages)
    activate ScyllaDB
    ScyllaDB-->>CommandHandler: Success Response
    deactivate ScyllaDB
    
    Note over ScyllaDB: Event saved to event_store<br/>Outbox entry created

    ScyllaDB->>CDCProcessor: CDC Change Stream
    activate CDCProcessor
    CDCProcessor->>Redpanda: Publish Event
    activate Redpanda
    Redpanda-->>Downstream: Event Available
    deactivate CDCProcessor
    deactivate Redpanda
    deactivate CommandHandler
```

**Step-by-Step Process:**

1. **User Request** → HTTP Handler (Not shown in this project)
   ```
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
   ```

2. **Command Processing** → OrderCommandHandler
   ```
   ┌──────────────────────────────────────────────────────┐
   │              OrderCommandHandler::handle()           │
   │                                                      │
   │  1. Load OrderAggregate from events                 │
   │  2. Validate CreateOrder command                    │
   │  3. Apply command to create OrderCreatedEvent       │
   │  4. Call EventStore.append_events()                 │
   │     ├─► Prepare batch                               │
   │     ├─► Add to event_store                          │
   │     ├─► Add to outbox_messages                      │
   │     └─► Execute atomically                          │
   └──────────────────┬───────────────────────────────────┘
   ```

3. **Database Write** → ScyllaDB
   ```
   ┌──────────────────────────────────────────────────────┐
   │                   ScyllaDB                           │
   │                                                      │
   │  BEGIN BATCH;                                        │
   │    INSERT INTO event_store                           │
   │      (aggregate_id, sequence_number, event_id,      │
   │       event_type, event_version, event_data, ...)   │
   │    VALUES (?, ?, ?, 'OrderCreated', 1, ...);        │
   │                                                      │
   │    INSERT INTO outbox_messages                       │
   │      (id, aggregate_id, event_type, payload, ...)   │
   │    VALUES (?, ?, 'OrderCreated', '{"order_id":...}',│
   │             ...);                                    │
   │  APPLY BATCH;                                        │
   │                                                      │
   │  ✅ Both writes succeed atomically                   │
   └──────────────────┬───────────────────────────────────┘
   ```

4. **CDC Capture** → CDC Hidden Log Tables
   ```
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
   ```

5. **Consumer Processing** → OutboxCDCConsumer
   ```
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
   └──────────────────┬───────────────────────────────────┘
   ```

**Latency Breakdown (Happy Path):**
```
Operation                      Latency
─────────────────────────────────────────────────────────
1. User request → Command Handler  ~1ms
2. Command processing            ~0.5ms
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

```mermaid
graph LR
    subgraph "Scenario 1: Redpanda Temporary Outage"
        S1_T0["T0: Order created, batch written SUCCESS"] --> S1_T1["T1: CDC captures change SUCCESS"]
        S1_T1 --> S1_T2["T2: Attempt 1: Redpanda refused FAIL"]
        S1_T2 --> S1_T3["T3: Wait 100ms"]
        S1_T3 --> S1_T4["T4: Attempt 2: Still down FAIL"]
        S1_T4 --> S1_T5["T5: Wait 200ms"]
        S1_T5 --> S1_T6["T6: Attempt 3: Still down FAIL"]
        S1_T6 --> S1_T7["T7: Wait 400ms"]
        S1_T7 --> S1_T8["T8: Attempt 4: Redpanda recovers! SUCCESS"]
        S1_T8 --> S1_RESULT["Result: Event published after 700ms"]
    end
    
    subgraph "Scenario 2: Redpanda Extended Outage"
        S2_T0["T0: Order created SUCCESS"] --> S2_T1["T1: CDC captures SUCCESS"]
        S2_T1 --> S2_T2["T2-T6: All 5 attempts fail FAIL"]
        S2_T2 --> S2_T7["T7: Message to DLQ SUCCESS"]
        S2_T7 --> S2_T8["T8: Alert triggered"]
        S2_T8 --> S2_T9["T9: Engineer investigates"]
        S2_T9 --> S2_T10["T10: Redpanda fixed"]
        S2_T10 --> S2_T11["T11: DLQ messages replayed SUCCESS"]
    end
```

**Impact Summary:**
```
Scenario 1: Redpanda Temporary Outage
───────
- Order saved: YES
- Event eventually published: YES
- User notified: YES (order confirmed)
- Downstreams notified: YES (delayed ~700ms)

Scenario 2: Redpanda Extended Outage
───────
- Order saved: YES
- Event in DLQ: YES
- User notified: YES (order confirmed)
- Downstreams notified: NO (waiting on manual replay)
- SLA: Degraded, but no data loss

Scenario 3: ScyllaDB Write Failure
───────
- Order saved: NO
- Event published: NO
- User notified: NO (received error response)
- System state: Consistent (nothing saved) YES

Scenario 4: Partial CDC Processing Failure
───────
- Orders saved: YES (all 100)
- Events published: YES (50 immediate + 50 after recovery)
- Circuit breaker prevented cascading failure YES
- Total delay for last 50: ~30s

Scenario 5: Consumer Crash
───────
- Orders saved: YES
- Events eventually published: YES
- Possible duplicates: Handled by idempotency YES
- Downtime: ~1-2 seconds
```

**Recovery Decision Tree:**
```mermaid
flowchart TD
    A[Failure Detected] --> B{Transient Error?}
    B -->|No| C[Permanent Failure<br/>Send to DLQ]
    B -->|Yes| D{Attempts < Max?}
    D -->|No| C
    D -->|Yes| E[Wait with Backoff]
    E --> F[Retry Operation]
    F --> A
    C --> G[Alert On-Call]
    G --> H[Manual Recovery]
    
    style A fill:#ffcdd2
    style C fill:#ffcdd2
    style G fill:#fff3e0
    style H fill:#fff3e0
```

**Note that most of the numbers in this document are experimental and would vary based on environment, network, and load.**
---

## Documentation Links

- [Return to Documentation Index](INDEX.md) - Back to the main documentation index
- [Return to README](../README.md) - Back to main project page
- [Main Tutorial](TUTORIAL.md) - Complete Event Sourcing tutorial with rich diagrams
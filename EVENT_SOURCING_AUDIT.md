# Event Sourcing Folder - Organization & Naming Audit

## Current State Analysis

### 📁 Current File Structure

```
src/event_sourcing/
├── aggregate.rs              # Order-specific aggregate (DOMAIN-COUPLED!)
├── event_store.rs            # Generic EventStore + OrderCommandHandler (MIXED!)
├── events.rs                 # Order-specific events (DOMAIN-COUPLED!)
├── mod.rs                    # Module exports
├── projection_consumer.rs    # CDC consumer for projections
├── projections.rs            # Order-specific projections (DOMAIN-COUPLED!)
└── snapshot.rs               # Snapshot store + OptimizedEventStore
```

### 🔴 **CRITICAL ISSUES FOUND**

---

## Issue 1: Domain-Specific Code in Infrastructure Layer

### ❌ **Problem**: "Order" domain littered throughout infrastructure

| File | Order-Specific Types | Should Be Generic |
|------|---------------------|-------------------|
| `aggregate.rs` | `OrderAggregate`, `OrderCommand`, `OrderEvent`, `OrderStatus`, `OrderError` | ✅ Generic `Aggregate` trait exists but... |
| `events.rs` | `OrderItem`, `OrderCreated`, `OrderConfirmed`, `OrderShipped`, etc. | ❌ All events are Order-specific |
| `event_store.rs` | `OrderCommandHandler` (line 232) | ❌ Should be generic `CommandHandler<A>` |
| `projections.rs` | `OrderReadModelProjection`, `OrdersByCustomerProjection` | ❌ All projections are Order-specific |
| `snapshot.rs` | Uses `OrderAggregate` directly (line 8, 36, 67, etc.) | ❌ Should be generic `Aggregate` |

**Impact**:
- 🔴 **Cannot add new aggregates** (e.g., Customer, Product, Payment) without modifying infrastructure
- 🔴 **Violates Open-Closed Principle** (OCP)
- 🔴 **Tight coupling** between domain and infrastructure

---

## Issue 2: Mixed Responsibilities in Files

### ❌ **event_store.rs** - Contains TWO different things:

1. **Generic EventStore** (lines 24-227)
   - ✅ Good: Generic over events
   - ✅ Works with `EventEnvelope<OrderEvent>`

2. **Order-Specific CommandHandler** (lines 232-307)
   - ❌ Bad: Hardcoded to `OrderCommand` and `OrderAggregate`
   - ❌ Should be in domain layer, not infrastructure

**Problem**: Infrastructure mixed with domain logic

---

### ❌ **snapshot.rs** - Tightly coupled to OrderAggregate

```rust
use super::aggregate::{OrderAggregate, Aggregate};  // Line 8

pub struct SnapshotStore {
    pub async fn save_snapshot(&self, aggregate: &OrderAggregate, ...) // Line 36
    pub async fn load_latest_snapshot(&self, aggregate_id: Uuid) -> Result<Option<(OrderAggregate, i64)>>
}
```

**Problem**: SnapshotStore should work with ANY aggregate, not just OrderAggregate

---

### ❌ **projection_consumer.rs** - Duplicate Projection trait

```rust
pub trait Projection: Send + Sync {  // Line 30
    fn name(&self) -> &str;
    async fn handle_event(&self, event: &EventEnvelope<OrderEvent>) -> Result<()>;
    // ...
}
```

**Problem**:
- Same trait defined in `projections.rs` (line 24)
- Hardcoded to `OrderEvent`
- Should be generic over event types

---

## Issue 3: Poor Semantic Naming

### ❌ Unclear File Names

| Current Name | What It Actually Contains | Better Name |
|--------------|---------------------------|-------------|
| `aggregate.rs` | Order aggregate + Command + Event enums | `order/aggregate.rs` or `domain/order.rs` |
| `events.rs` | Order events + EventEnvelope | Split: `core/envelope.rs` + `order/events.rs` |
| `event_store.rs` | EventStore + OrderCommandHandler | Split: `store.rs` + `order/command_handler.rs` |
| `projections.rs` | Projection trait + Order projections | Split: `core/projection.rs` + `order/projections.rs` |
| `snapshot.rs` | SnapshotStore + OptimizedEventStore | `store/snapshot.rs` |

---

## Issue 4: No Clear Separation of Concerns

### Current (Flat Structure - Everything Mixed)

```
event_sourcing/
├── aggregate.rs        ← DOMAIN (Order-specific)
├── events.rs           ← MIXED (Generic trait + Order events)
├── event_store.rs      ← MIXED (Infrastructure + Order handler)
├── projections.rs      ← MIXED (Generic trait + Order projections)
├── snapshot.rs         ← INFRASTRUCTURE (but coupled to Order)
└── projection_consumer.rs  ← INFRASTRUCTURE (but coupled to Order)
```

**Problems**:
- 🔴 No clear boundary between infrastructure and domain
- 🔴 Cannot reuse infrastructure for new aggregates
- 🔴 Difficult to test in isolation
- 🔴 Violates Single Responsibility Principle

---

## Issue 5: Violates Open-Closed Principle (OCP)

### ❌ Adding a New Aggregate (e.g., "Customer") Requires:

1. ✅ Create `CustomerAggregate` (implement `Aggregate` trait) - OK
2. ✅ Create `CustomerEvent` enum - OK
3. ❌ **Modify** `EventStore` to handle `CustomerEvent` - WRONG!
4. ❌ **Modify** `SnapshotStore` to work with `CustomerAggregate` - WRONG!
5. ❌ **Create** new `CustomerCommandHandler` by copy-pasting - DUPLICATION!
6. ❌ **Modify** projection consumer to handle both Order and Customer - WRONG!

**Why OCP is violated**:
- Infrastructure code needs modification for each new aggregate
- No generic `CommandHandler<A: Aggregate>`
- No generic `SnapshotStore<A: Aggregate>`
- EventStore hardcoded to `OrderEvent`

---

## Proposed Refactoring

### 🎯 **Goal**: Clean separation between infrastructure and domain

### New Structure (Layered Architecture)

```
src/
├── event_sourcing/               # ← INFRASTRUCTURE LAYER (Generic, Reusable)
│   ├── mod.rs
│   │
│   ├── core/                     # ← Core abstractions
│   │   ├── mod.rs
│   │   ├── aggregate.rs          # Aggregate trait
│   │   ├── event.rs              # EventEnvelope, DomainEvent trait
│   │   ├── command_handler.rs    # Generic CommandHandler<A>
│   │   └── error.rs              # Event sourcing errors
│   │
│   ├── store/                    # ← Persistence layer
│   │   ├── mod.rs
│   │   ├── event_store.rs        # Generic EventStore<E>
│   │   ├── snapshot_store.rs     # Generic SnapshotStore<A>
│   │   └── repository.rs         # Generic Repository<A>
│   │
│   └── projections/              # ← Projection infrastructure
│       ├── mod.rs
│       ├── projection.rs         # Projection trait
│       ├── consumer.rs           # CDC consumer
│       └── manager.rs            # ProjectionManager
│
└── domain/                       # ← DOMAIN LAYER (Business Logic)
    ├── mod.rs
    │
    ├── order/                    # ← Order aggregate
    │   ├── mod.rs
    │   ├── aggregate.rs          # OrderAggregate
    │   ├── commands.rs           # OrderCommand enum
    │   ├── events.rs             # OrderEvent enum + specific events
    │   ├── errors.rs             # OrderError enum
    │   ├── projections.rs        # Order-specific projections
    │   └── value_objects.rs      # OrderItem, OrderStatus
    │
    ├── customer/                 # ← Future: Customer aggregate
    │   ├── mod.rs
    │   ├── aggregate.rs          # CustomerAggregate
    │   ├── commands.rs           # CustomerCommand enum
    │   └── events.rs             # CustomerEvent enum
    │
    └── product/                  # ← Future: Product aggregate
        └── ...
```

---

## Detailed File Breakdown

### 📁 **event_sourcing/core/** (Infrastructure - Generic)

#### `aggregate.rs`
```rust
/// Generic aggregate trait - works for ANY aggregate
pub trait Aggregate: Sized + Send + Sync {
    type Event: DomainEvent;
    type Command;
    type Error;

    fn aggregate_id(&self) -> Uuid;
    fn version(&self) -> i64;
    fn apply_first_event(event: &Self::Event) -> Result<Self, Self::Error>;
    fn apply_event(&mut self, event: &Self::Event) -> Result<(), Self::Error>;
    fn handle_command(&self, command: &Self::Command) -> Result<Vec<Self::Event>, Self::Error>;
}
```

#### `event.rs`
```rust
/// Generic event envelope - works for ANY event type
pub struct EventEnvelope<E: DomainEvent> {
    pub event_id: Uuid,
    pub aggregate_id: Uuid,
    pub sequence_number: i64,
    pub event_type: String,
    pub event_data: E,
    // ... metadata
}

pub trait DomainEvent: Serialize + for<'de> Deserialize<'de> + Clone {
    fn event_type() -> &'static str where Self: Sized;
}
```

#### `command_handler.rs`
```rust
/// Generic command handler - works for ANY aggregate!
pub struct CommandHandler<A: Aggregate> {
    event_store: Arc<EventStore<A::Event>>,
    _phantom: PhantomData<A>,
}

impl<A: Aggregate> CommandHandler<A> {
    pub async fn handle(
        &self,
        aggregate_id: Uuid,
        command: A::Command,
        correlation_id: Uuid,
    ) -> Result<i64> {
        // Load aggregate
        let aggregate = if self.event_store.aggregate_exists(aggregate_id).await? {
            self.event_store.load_aggregate::<A>(aggregate_id).await?
        } else {
            // Handle CreateOrder-like commands
        };

        // Handle command through aggregate
        let events = aggregate.handle_command(&command)?;

        // Persist events
        self.event_store.append_events(aggregate_id, aggregate.version(), events).await
    }
}
```

---

### 📁 **event_sourcing/store/** (Infrastructure - Generic)

#### `event_store.rs`
```rust
/// Generic event store - works with ANY event type
pub struct EventStore<E: DomainEvent> {
    session: Arc<Session>,
    _phantom: PhantomData<E>,
}

impl<E: DomainEvent> EventStore<E> {
    pub async fn append_events(
        &self,
        aggregate_id: Uuid,
        expected_version: i64,
        events: Vec<EventEnvelope<E>>,
    ) -> Result<i64> {
        // Generic implementation
    }

    pub async fn load_events(&self, aggregate_id: Uuid) -> Result<Vec<EventEnvelope<E>>> {
        // Generic implementation
    }

    pub async fn load_aggregate<A>(&self, aggregate_id: Uuid) -> Result<A>
    where
        A: Aggregate<Event = E>,
    {
        let events = self.load_events(aggregate_id).await?;
        A::load_from_events(events)
    }
}
```

#### `snapshot_store.rs`
```rust
/// Generic snapshot store - works with ANY aggregate
pub struct SnapshotStore<A: Aggregate> {
    session: Arc<Session>,
    _phantom: PhantomData<A>,
}

impl<A: Aggregate + Serialize + for<'de> Deserialize<'de>> SnapshotStore<A> {
    pub async fn save_snapshot(&self, aggregate: &A, event_count: i32) -> Result<()> {
        // Serialize any aggregate to JSON
    }

    pub async fn load_latest_snapshot(&self, aggregate_id: Uuid) -> Result<Option<(A, i64)>> {
        // Deserialize any aggregate from JSON
    }
}
```

---

### 📁 **event_sourcing/projections/** (Infrastructure - Generic)

#### `projection.rs`
```rust
/// Generic projection trait - can handle any event type
#[async_trait]
pub trait Projection<E: DomainEvent>: Send + Sync {
    fn name(&self) -> &str;
    async fn handle_event(&self, event: &EventEnvelope<E>) -> Result<()>;
    async fn get_offset(&self) -> Result<i64>;
    async fn save_offset(&self, sequence_number: i64, event_id: Uuid) -> Result<()>;
}

/// Projection manager - generic over event type
pub struct ProjectionManager<E: DomainEvent> {
    projections: Vec<Arc<dyn Projection<E>>>,
}
```

---

### 📁 **domain/order/** (Domain - Order-Specific)

#### `aggregate.rs`
```rust
pub struct OrderAggregate {
    pub id: Uuid,
    pub version: i64,
    pub customer_id: Uuid,
    pub items: Vec<OrderItem>,
    pub status: OrderStatus,
    // ...
}

impl Aggregate for OrderAggregate {
    type Event = OrderEvent;
    type Command = OrderCommand;
    type Error = OrderError;

    // Implementation specific to Order
}
```

#### `events.rs`
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum OrderEvent {
    Created(OrderCreated),
    Confirmed(OrderConfirmed),
    Shipped(OrderShipped),
    // ...
}

impl DomainEvent for OrderEvent {
    fn event_type() -> &'static str { "OrderEvent" }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderCreated {
    pub customer_id: Uuid,
    pub items: Vec<OrderItem>,
}
// ... other event structs
```

#### `projections.rs`
```rust
pub struct OrderReadModelProjection {
    session: Arc<Session>,
}

impl Projection<OrderEvent> for OrderReadModelProjection {
    fn name(&self) -> &str { "order_read_model" }

    async fn handle_event(&self, event: &EventEnvelope<OrderEvent>) -> Result<()> {
        match &event.event_data {
            OrderEvent::Created(e) => {
                // Update order_read_model table
            }
            // ... other events
        }
    }
}
```

---

## Benefits of Refactoring

### ✅ **1. Open-Closed Principle (OCP) Compliance**

Adding a new aggregate (e.g., Customer):

```rust
// 1. Define Customer domain (NO infrastructure changes!)
domain/customer/
├── aggregate.rs        impl Aggregate for CustomerAggregate
├── commands.rs         enum CustomerCommand
├── events.rs           enum CustomerEvent
└── projections.rs      impl Projection<CustomerEvent>

// 2. Use existing infrastructure (NO MODIFICATIONS!)
let customer_store = EventStore::<CustomerEvent>::new(session);
let customer_handler = CommandHandler::<CustomerAggregate>::new(customer_store);
let customer_snapshots = SnapshotStore::<CustomerAggregate>::new(session);
```

**NO infrastructure code modified!** ✅

---

### ✅ **2. Clear Separation of Concerns**

| Layer | Responsibility | Changes When... |
|-------|----------------|-----------------|
| **Infrastructure** (`event_sourcing/`) | Generic persistence, CDC, projections | Database tech changes |
| **Domain** (`domain/order/`) | Business logic, rules | Business requirements change |

---

### ✅ **3. Reusability**

- EventStore can store ANY event type
- SnapshotStore can snapshot ANY aggregate
- CommandHandler can handle ANY aggregate
- ProjectionManager can manage ANY projections

---

### ✅ **4. Testability**

```rust
#[cfg(test)]
mod tests {
    // Test infrastructure with mock aggregates
    struct TestAggregate { /* ... */ }
    impl Aggregate for TestAggregate { /* ... */ }

    #[test]
    fn test_event_store() {
        let store = EventStore::<TestEvent>::new(...);
        // Test generic event store with test events
    }
}
```

---

### ✅ **5. No Code Duplication**

Current:
- `OrderCommandHandler` hardcoded
- Need `CustomerCommandHandler`, `ProductCommandHandler`, etc. (copy-paste!)

After refactoring:
- ONE `CommandHandler<A>` works for all aggregates ✅

---

## Migration Path

### Phase 1: Extract Core Abstractions (No Breaking Changes)
1. Create `event_sourcing/core/` directory
2. Move `Aggregate` trait to `core/aggregate.rs`
3. Move `DomainEvent` trait to `core/event.rs`
4. Move `EventEnvelope` to `core/event.rs`
5. Update imports (backward compatible)

### Phase 2: Make Infrastructure Generic
1. Convert `EventStore` to `EventStore<E: DomainEvent>`
2. Convert `SnapshotStore` to `SnapshotStore<A: Aggregate>`
3. Create generic `CommandHandler<A: Aggregate>`
4. Convert `Projection` trait to `Projection<E: DomainEvent>`

### Phase 3: Extract Domain Layer
1. Create `domain/order/` directory
2. Move Order-specific types from `aggregate.rs`
3. Move Order-specific events from `events.rs`
4. Move Order projections from `projections.rs`
5. Delete `OrderCommandHandler` (replaced by generic)

### Phase 4: Cleanup
1. Remove unused Order-specific infrastructure code
2. Update `mod.rs` exports
3. Update `main.rs` imports
4. Update tests

---

## Summary of Issues

| # | Issue | Severity | Impact |
|---|-------|----------|--------|
| 1 | Domain code in infrastructure | 🔴 Critical | Cannot add new aggregates |
| 2 | Mixed responsibilities | 🔴 Critical | Hard to maintain |
| 3 | Poor file naming | 🟡 Medium | Confusing structure |
| 4 | No separation of concerns | 🔴 Critical | Tight coupling |
| 5 | Violates OCP | 🔴 Critical | Not extensible |
| 6 | Duplicate trait definitions | 🟡 Medium | Confusion |
| 7 | Hardcoded to OrderAggregate | 🔴 Critical | Not reusable |

---

## Recommendation

**Priority**: 🔴 **HIGH** - Refactor ASAP before adding more features

**Effort**: Medium (2-3 days)

**Risk**: Low (can be done incrementally with backward compatibility)

**Benefit**:
- ✅ Can add new aggregates without touching infrastructure
- ✅ Reusable event sourcing framework
- ✅ Clean architecture
- ✅ Easier testing
- ✅ Better maintainability

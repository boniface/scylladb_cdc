# Event Sourcing Refactoring - Migration Plan

## ✅ Phase 1: COMPLETED - Core Abstractions Created

### Created Files:
```
src/event_sourcing/
├── core/
│   ├── mod.rs                 ✅ Created
│   ├── aggregate.rs           ✅ Created (generic Aggregate trait)
│   ├── event.rs               ✅ Created (generic EventEnvelope<E>, DomainEvent)
├── store/
│   ├── mod.rs                 ✅ Created
│   └── event_store.rs         ✅ Created (generic EventStore<E>)
```

### What These Files Provide:
1. **Generic Aggregate Trait** - Works with ANY aggregate
2. **Generic EventEnvelope** - Works with ANY event type
3. **Generic EventStore** - Works with ANY event type
4. **Separation** - Infrastructure is now decoupled from domain

---

## 🔄 Phase 2: IN PROGRESS - Extract Order Domain

### Next Steps:

#### Step 2.1: Create domain/order directory
```bash
mkdir -p src/domain/order
```

#### Step 2.2: Extract Order-specific types to domain/order/

**Create `src/domain/order/value_objects.rs`:**
```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrderItem {
    pub product_id: Uuid,
    pub quantity: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderStatus {
    Created,
    Confirmed,
    Shipped,
    Delivered,
    Cancelled,
}
```

**Create `src/domain/order/events.rs`:**
- Move all `Order*` event structs from `event_sourcing/events.rs`
- Keep `OrderEvent` enum
- Import from `event_sourcing::core`

**Create `src/domain/order/commands.rs`:**
- Move `OrderCommand` enum from `event_sourcing/aggregate.rs`

**Create `src/domain/order/errors.rs`:**
- Move `OrderError` enum from `event_sourcing/aggregate.rs`

**Create `src/domain/order/aggregate.rs`:**
- Move `OrderAggregate` struct from `event_sourcing/aggregate.rs`
- Implement `Aggregate` trait from `event_sourcing::core`

**Create `src/domain/order/command_handler.rs`:**
- Create `OrderCommandHandler` using generic `EventStore<OrderEvent>`
- Move logic from `event_sourcing/event_store.rs`

**Create `src/domain/order/projections.rs`:**
- Move `OrderReadModelProjection` from `event_sourcing/projections.rs`
- Move `OrdersByCustomerProjection` from `event_sourcing/projections.rs`

**Create `src/domain/order/mod.rs`:**
```rust
pub mod value_objects;
pub mod events;
pub mod commands;
pub mod errors;
pub mod aggregate;
pub mod command_handler;
pub mod projections;

pub use value_objects::*;
pub use events::*;
pub use commands::*;
pub use errors::*;
pub use aggregate::*;
pub use command_handler::*;
pub use projections::*;
```

---

## 🔄 Phase 3: Update event_sourcing/mod.rs

**Update `src/event_sourcing/mod.rs`:**
```rust
// Core abstractions (generic, reusable)
pub mod core;
pub mod store;

// Legacy files (to be removed after migration)
pub mod events;      // Keep temporarily for backward compatibility
pub mod aggregate;   // Keep temporarily for backward compatibility
pub mod event_store; // Keep temporarily for backward compatibility

pub mod snapshot;
pub mod projections;
pub mod projection_consumer;

// Re-export core types
pub use core::*;
pub use store::*;

// Legacy re-exports (for backward compatibility during migration)
pub use events::*;
pub use aggregate::*;
pub use event_store::*;
pub use snapshot::*;
pub use projections::*;
pub use projection_consumer::*;
```

---

## 🔄 Phase 4: Update main.rs to Use New Structure

**Current main.rs:**
```rust
use event_sourcing::{EventStore, OrderCommandHandler, OrderCommand, OrderItem};
```

**After refactoring:**
```rust
use event_sourcing::store::EventStore;
use domain::order::{
    OrderCommandHandler,
    OrderCommand,
    OrderEvent,
    OrderItem,
};

// Create Order event store
let order_event_store = Arc::new(
    EventStore::<OrderEvent>::new(session.clone(), "Order", "order-events")
);

// Create Order command handler
let order_command_handler = Arc::new(
    OrderCommandHandler::new(order_event_store.clone())
);
```

---

## 🔄 Phase 5: Clean Up Old Files

**After everything works, delete:**
```bash
# Delete old files (all code moved to domain/order/)
rm src/event_sourcing/aggregate.rs
rm src/event_sourcing/events.rs
rm src/event_sourcing/event_store.rs

# Keep these (they're now generic):
# - src/event_sourcing/core/*
# - src/event_sourcing/store/*
# - src/event_sourcing/snapshot.rs (needs to be made generic)
# - src/event_sourcing/projections.rs (extract generic Projection trait)
# - src/event_sourcing/projection_consumer.rs (make generic)
```

---

## 📊 Benefits After Migration

### Before (Current State):
```rust
// ❌ Adding Customer aggregate requires modifying infrastructure

// 1. Create CustomerAggregate (can't reuse EventStore)
// 2. Modify EventStore to handle CustomerEvent
// 3. Copy-paste OrderCommandHandler to CustomerCommandHandler
// 4. Modify projections infrastructure
```

### After (Refactored):
```rust
// ✅ Adding Customer aggregate - just create domain code!

// 1. Create domain/customer/ directory
domain/customer/
├── aggregate.rs         // impl Aggregate for CustomerAggregate
├── events.rs            // enum CustomerEvent
├── commands.rs          // enum CustomerCommand
├── command_handler.rs   // CustomerCommandHandler
└── projections.rs       // Customer projections

// 2. Use existing infrastructure (NO MODIFICATIONS!)
let customer_store = EventStore::<CustomerEvent>::new(
    session, "Customer", "customer-events"
);
let customer_handler = CustomerCommandHandler::new(customer_store);

// ✅ Infrastructure unchanged!
// ✅ OCP compliant!
// ✅ Fully reusable!
```

---

## 🎯 Current Status

### ✅ Completed:
- [x] Created `event_sourcing/core/` with generic abstractions
- [x] Created `event_sourcing/store/` with generic EventStore
- [x] All new code is generic and reusable

### 🔄 In Progress:
- [ ] Extract Order domain to `domain/order/`
- [ ] Update main.rs to use new structure
- [ ] Make snapshot.rs generic
- [ ] Extract generic Projection trait
- [ ] Update projections.rs and projection_consumer.rs

### ⏳ Pending:
- [ ] Delete old files
- [ ] Update all imports
- [ ] Run tests
- [ ] Update documentation

---

## 🚀 How to Continue

### Option 1: Manual Migration (Recommended)
Follow steps in Phase 2-5 above. This gives you full control and understanding.

### Option 2: Automated Script
I can create a script to automate the file moves and updates.

### Option 3: Incremental Migration
Keep both old and new structures temporarily, migrate one component at a time.

---

## 📝 Testing Strategy

After each phase:

```bash
# 1. Check compilation
cargo build

# 2. Run tests
cargo test

# 3. Run application
make reset
make run

# 4. Verify CDC and projections work
# Check logs for event processing
```

---

## ⚠️ Important Notes

1. **Backward Compatibility**: Keep old files temporarily and re-export from them
2. **Incremental**: Migrate one aggregate at a time
3. **Testing**: Test after each phase
4. **Rollback Plan**: Use git branches for safety

---

## 📖 Next Steps

**Immediate**:
1. Review the created `core/` and `store/` modules
2. Decide on migration approach (manual/automated/incremental)
3. Create `domain/order/` directory
4. Move Order-specific code file by file

**After Order Migration**:
1. Test thoroughly
2. Delete old files
3. Clean up imports
4. Document new structure

**Future**:
1. Add Customer aggregate (to verify extensibility)
2. Add Product aggregate
3. Enjoy clean, extensible architecture!

---

## 🎓 Architecture Principles Applied

### Single Responsibility Principle (SRP)
- ✅ `event_sourcing/`: Infrastructure only
- ✅ `domain/order/`: Order business logic only

### Open-Closed Principle (OCP)
- ✅ Open for extension (new aggregates)
- ✅ Closed for modification (infrastructure unchanged)

### Dependency Inversion Principle (DIP)
- ✅ Infrastructure depends on abstractions (`Aggregate`, `DomainEvent`)
- ✅ Domain implements abstractions

### Separation of Concerns
- ✅ Clear boundaries between layers
- ✅ Infrastructure is reusable
- ✅ Domain is isolated

---

This migration sets you up for a clean, maintainable, extensible event sourcing architecture! 🎉

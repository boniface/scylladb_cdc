# Contributing to ScyllaDB Event Sourcing CDC

Thank you for your interest in contributing! This document provides guidelines for contributing to the project.

## Development Setup

### Prerequisites

- Rust 1.70+ with cargo
- Docker and Docker Compose
- ScyllaDB knowledge (helpful)
- Event Sourcing concepts (helpful)

### Local Setup

```bash
# Clone repository
git clone <repo-url>
cd scylladb_cdc

# Start infrastructure
docker-compose up -d

# Initialize schema
cqlsh -f src/db/schema.cql

# Run tests
cargo test

# Run application
cargo run
```

## Project Structure

```
src/
├── domain/              # Domain aggregates (Order, Customer)
│   ├── order/          # Order aggregate with events/commands
│   └── customer/       # Customer aggregate with events/commands
├── event_sourcing/     # Event sourcing infrastructure
│   ├── core/          # Generic aggregate and event traits
│   └── store/         # EventStore implementation
├── actors/             # Actor infrastructure
│   ├── core/          # Actor abstractions
│   └── infrastructure/# CDC processor, DLQ, health monitor
├── messaging/          # Redpanda/Kafka integration
├── utils/              # Circuit breaker, retry logic
├── metrics/            # Prometheus metrics
└── db/                 # Database schema
```

## Contribution Guidelines

### Code Style

- Follow Rust standard conventions
- Run `cargo fmt` before committing
- Run `cargo clippy` and fix warnings
- Add documentation for public APIs
- Write clear commit messages

### Testing

**Required for all contributions:**

1. **Unit Tests** - Test business logic in isolation
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn test_order_creation_with_valid_items() {
           // Test implementation
       }
   }
   ```

2. **Integration Tests** - Test with real database
   - Use testcontainers for ScyllaDB
   - Test complete workflows
   - Located in `tests/` directory

3. **Test Coverage** - See `docs/TEST_AUDIT.md` for current status
   - Target: 80%+ for business logic
   - Required: All new features must have tests

### Adding New Aggregates

When adding a new domain aggregate (e.g., Product, Payment):

1. Create directory: `src/domain/<aggregate_name>/`
2. Required files:
   ```
   domain/<aggregate>/
   ├── mod.rs              # Module exports
   ├── value_objects.rs    # Domain value objects
   ├── events.rs           # Domain events (implement DomainEvent)
   ├── commands.rs         # Commands (enum)
   ├── errors.rs           # Business rule errors (thiserror)
   ├── aggregate.rs        # Aggregate root (implement Aggregate trait)
   └── command_handler.rs  # Command handler
   ```

3. Implement traits:
   - `DomainEvent` for events
   - `Aggregate` for aggregate root
   - Include `load_from_events` with proper version tracking

4. Add tests for:
   - All commands
   - All events
   - State transitions
   - Business rule validation
   - Concurrency control

### Code Review Process

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes
4. Add tests
5. Run full test suite: `cargo test`
6. Format code: `cargo fmt`
7. Check for issues: `cargo clippy`
8. Commit with clear message
9. Push to your fork
10. Create Pull Request

### Pull Request Guidelines

**PR Title Format:**
- `feat: Add customer payment methods`
- `fix: Resolve concurrency issue in EventStore`
- `docs: Update architecture documentation`
- `test: Add unit tests for Order aggregate`
- `refactor: Simplify CDC processor logic`

**PR Description Should Include:**
- What: Brief description of changes
- Why: Motivation for the change
- How: Technical approach
- Testing: What tests were added/modified
- Breaking Changes: If any

**Example:**
```markdown
## What
Add payment method management to Customer aggregate

## Why
Customers need to store multiple payment methods for orders

## How
- Added PaymentMethod value object
- Added AddPaymentMethod/RemovePaymentMethod commands
- Added corresponding events
- Updated aggregate business logic

## Testing
- Added 10 unit tests for payment method operations
- Added integration test for full payment method lifecycle
- All existing tests still pass

## Breaking Changes
None
```

## Areas for Contribution

### High Priority

1. **Tests** (see `docs/TEST_AUDIT.md`)
   - Order aggregate tests (15-20 tests needed)
   - Customer aggregate tests (20-25 tests needed)
   - EventStore tests (10-15 tests needed)
   - Actor tests (10-15 tests needed)

2. **Projections**
   - Implement projection consumers
   - Add read model updates
   - Add projection rebuild capability

3. **Snapshots**
   - Implement snapshot creation (every 100 events)
   - Add snapshot loading in EventStore
   - Add cleanup for old snapshots

### Medium Priority

4. **Event Upcasting**
   - Schema versioning support
   - Event migration scripts
   - Backward compatibility

5. **Sagas**
   - Multi-aggregate transaction patterns
   - Saga orchestration
   - Compensation logic

6. **Monitoring**
   - Additional Prometheus metrics
   - Grafana dashboards
   - Alerting rules

### Low Priority

7. **Performance**
   - Benchmark tests
   - Optimization opportunities
   - Load testing

8. **Documentation**
   - Additional examples
   - Video tutorials
   - Blog posts

## Questions?

- Open an issue for bugs or feature requests
- Start a discussion for questions or ideas
- Check existing issues before creating new ones

## License

By contributing, you agree that your contributions will be licensed under the same license as the project (MIT License).

---

Thank you for contributing! 🎉

.PHONY: help build test clean dev schema reset

help:
	@echo "ScyllaDB Event Sourcing with CDC - Available Commands"
	@echo "====================================================="
	@echo "make build            - Build the application"
	@echo "make test             - Run unit tests"
	@echo "make dev              - Start services and run app"
	@echo "make reset            - Clean restart (removes all data)"
	@echo "make schema           - Initialize database schema"
	@echo "make metrics          - View Prometheus metrics"
	@echo "make clean            - Stop services and clean up"

build:
	@echo " Building application..."
	cargo build --release

test:
	@echo " Running unit tests..."
	cargo test

dev:
	@echo " Starting development environment..."
	@docker-compose up -d
	@echo " Waiting for ScyllaDB to be ready..."
	@sleep 25
	@$(MAKE) schema
	@echo "✅ Services ready! Starting application..."
	@RUST_LOG=info cargo run

reset:
	@echo "🔄 Resetting environment (clean start)..."
	@docker-compose down -v
	@echo " Starting fresh containers..."
	@docker-compose up -d
	@echo " Waiting for ScyllaDB to be ready..."
	@sleep 25
	@$(MAKE) schema
	@echo "✅ Environment reset complete!"
	@echo ""
	@echo "Now run: make run"

run:
	@echo " Starting application..."
	@RUST_LOG=info cargo run

schema:
	@echo " Initializing Event Sourcing schema..."
	@docker exec $$(docker-compose ps -q scylla) cqlsh -f /schema/schema.cql 2>&1 | grep -v "already exists" || true
	@echo "✅ Schema initialized"

metrics:
	@echo " Fetching Prometheus metrics..."
	@echo ""
	@echo "=== Event Store Metrics ==="
	@curl -s http://localhost:9090/metrics | grep "event_" || echo "No event metrics yet"
	@echo ""
	@echo "=== CDC Processing Metrics ==="
	@curl -s http://localhost:9090/metrics | grep "cdc_events" || echo "No CDC metrics yet"
	@echo ""
	@echo "=== Retry Metrics ==="
	@curl -s http://localhost:9090/metrics | grep "retry_" || echo "No retry metrics yet"
	@echo ""
	@echo "=== DLQ Metrics ==="
	@curl -s http://localhost:9090/metrics | grep "dlq_" || echo "No DLQ metrics yet"
	@echo ""
	@echo "=== Circuit Breaker Metrics ==="
	@curl -s http://localhost:9090/metrics | grep "circuit_breaker" || echo "No circuit breaker metrics yet"
	@echo ""

clean:
	@echo " Cleaning up..."
	@docker-compose down -v
	@cargo clean
	@echo "✅ Cleanup complete"

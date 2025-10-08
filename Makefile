.PHONY: help build test clean dev integration-test metrics

help:
	@echo "ScyllaDB CDC Outbox Pattern - Available Commands"
	@echo "================================================"
	@echo "make build            - Build the application"
	@echo "make test             - Run unit tests"
	@echo "make integration-test - Run full integration tests"
	@echo "make dev              - Start services and run app"
	@echo "make metrics          - View Prometheus metrics"
	@echo "make clean            - Stop services and clean up"
	@echo "make schema           - Initialize database schema"

build:
	@echo "🔨 Building application..."
	cargo build --release

test:
	@echo "🧪 Running unit tests..."
	cargo test

integration-test:
	@echo "🧪 Running integration tests..."
	./tests/integration_test.sh

dev:
	@echo "🚀 Starting development environment..."
	docker-compose up -d
	@echo "⏳ Waiting for services to be ready..."
	@sleep 15
	@$(MAKE) schema
	@echo "✅ Services ready! Starting application..."
	RUST_LOG=debug cargo run

schema:
	@echo "📊 Initializing database schema..."
	@docker exec $$(docker-compose ps -q scylla) cqlsh -e "\
		CREATE KEYSPACE IF NOT EXISTS orders_ks \
		WITH REPLICATION = {'class': 'SimpleStrategy', 'replication_factor': 1}; \
		USE orders_ks; \
		CREATE TABLE IF NOT EXISTS orders ( \
			id UUID PRIMARY KEY, \
			customer_id UUID, \
			items TEXT, \
			status TEXT, \
			created_at TIMESTAMP, \
			updated_at TIMESTAMP \
		); \
		CREATE TABLE IF NOT EXISTS outbox_messages ( \
			id UUID PRIMARY KEY, \
			aggregate_id UUID, \
			event_type TEXT, \
			payload TEXT, \
			created_at TIMESTAMP \
		) WITH cdc = {'enabled': true, 'preimage': false, 'postimage': true, 'ttl': 86400}; \
		CREATE TABLE IF NOT EXISTS dead_letter_queue ( \
			id UUID PRIMARY KEY, \
			aggregate_id UUID, \
			event_type TEXT, \
			payload TEXT, \
			error_message TEXT, \
			failure_count INT, \
			first_failed_at TIMESTAMP, \
			last_failed_at TIMESTAMP, \
			created_at TIMESTAMP \
		); \
		CREATE INDEX IF NOT EXISTS idx_dlq_event_type ON dead_letter_queue (event_type); \
		CREATE INDEX IF NOT EXISTS idx_dlq_aggregate_id ON dead_letter_queue (aggregate_id); \
		CREATE INDEX IF NOT EXISTS idx_dlq_last_failed_at ON dead_letter_queue (last_failed_at);" 2>/dev/null
	@echo "✅ Schema initialized"

metrics:
	@echo "📊 Fetching Prometheus metrics..."
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
	@echo "🧹 Cleaning up..."
	docker-compose down -v
	cargo clean
	@echo "✅ Cleanup complete"

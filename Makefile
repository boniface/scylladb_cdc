.PHONY: help build test clean dev schema reset run start stop logs status

# Application variables
APP_NAME = scylladb_cdc
PID_FILE = .app.pid
LOG_FILE = app.log

help:
	@echo "ScyllaDB Event Sourcing with CDC - Available Commands"
	@echo "======================================================"
	@echo ""
	@echo "🚀 Quick Start:"
	@echo "  make dev              - Start services and run app interactively"
	@echo "  make start            - Start services and run app in background"
	@echo "  make stop             - Stop background application"
	@echo ""
	@echo "📋 Build & Test:"
	@echo "  make build            - Build the application"
	@echo "  make test             - Run unit tests"
	@echo ""
	@echo "🔄 Database:"
	@echo "  make reset            - Clean restart (removes all data)"
	@echo "  make schema           - Initialize database schema"
	@echo ""
	@echo "📊 Monitoring:"
	@echo "  make metrics          - View Prometheus metrics"
	@echo "  make logs             - Tail application logs (background mode)"
	@echo "  make status           - Check if application is running"
	@echo ""
	@echo "🧹 Cleanup:"
	@echo "  make clean            - Stop services and clean up"
	@echo ""
	@echo "💡 Direct Run:"
	@echo "  make run              - Run app (assumes services already running)"
	@echo ""

build:
	@echo " Building application..."
	cargo build --release

test:
	@echo " Running unit tests..."
	cargo test

dev:
	@echo "🚀 Starting development environment..."
	@docker-compose up -d
	@echo "⏳ Waiting for ScyllaDB to be ready..."
	@sleep 25
	@$(MAKE) schema
	@echo "✅ Services ready! Starting application in interactive mode..."
	@echo ""
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
	@echo "🚀 Starting application in interactive mode..."
	@echo "   (Press Ctrl+C to stop)"
	@echo ""
	@RUST_LOG=info cargo run

start:
	@if [ -f $(PID_FILE) ]; then \
		echo "⚠️  Application may already be running (PID file exists)"; \
		echo "   Run 'make stop' first or check 'make status'"; \
		exit 1; \
	fi
	@echo "🚀 Starting services..."
	@docker-compose up -d
	@echo "⏳ Waiting for ScyllaDB to be ready..."
	@sleep 25
	@$(MAKE) schema
	@echo "🚀 Starting application in background..."
	@RUST_LOG=info cargo run > $(LOG_FILE) 2>&1 & echo $$! > $(PID_FILE)
	@sleep 3
	@if ps -p $$(cat $(PID_FILE)) > /dev/null 2>&1; then \
		echo "✅ Application started successfully!"; \
		echo ""; \
		echo "📊 Available endpoints:"; \
		echo "   Metrics:      http://localhost:9090/metrics"; \
		echo "   Health:       http://localhost:9090/health"; \
		echo ""; \
		echo "💡 Useful commands:"; \
		echo "   make logs     - View application logs"; \
		echo "   make metrics  - Check metrics"; \
		echo "   make status   - Check if running"; \
		echo "   make stop     - Stop application"; \
		echo ""; \
	else \
		echo "❌ Application failed to start. Check logs:"; \
		echo "   tail -50 $(LOG_FILE)"; \
		rm -f $(PID_FILE); \
		exit 1; \
	fi

stop:
	@if [ ! -f $(PID_FILE) ]; then \
		echo "⚠️  No PID file found. Application may not be running."; \
		echo "   Check with: make status"; \
		exit 0; \
	fi
	@echo "🛑 Stopping application (PID: $$(cat $(PID_FILE)))..."
	@kill -TERM $$(cat $(PID_FILE)) 2>/dev/null || echo "   Process not found, cleaning up..."
	@sleep 2
	@rm -f $(PID_FILE)
	@echo "✅ Application stopped"

logs:
	@if [ ! -f $(LOG_FILE) ]; then \
		echo "⚠️  No log file found. Application may not have been started in background mode."; \
		echo "   Use 'make start' to run in background, or 'make run' for interactive mode."; \
		exit 1; \
	fi
	@echo "📋 Tailing application logs (Ctrl+C to exit)..."
	@echo ""
	@tail -f $(LOG_FILE)

status:
	@echo "🔍 Checking application status..."
	@echo ""
	@if [ -f $(PID_FILE) ]; then \
		PID=$$(cat $(PID_FILE)); \
		if ps -p $$PID > /dev/null 2>&1; then \
			echo "✅ Application is RUNNING (PID: $$PID)"; \
			echo ""; \
			echo "📊 Endpoints:"; \
			curl -s http://localhost:9090/health 2>/dev/null && echo "" || echo "   ⚠️  Health endpoint not responding"; \
			echo ""; \
			echo "   Metrics: http://localhost:9090/metrics"; \
		else \
			echo "❌ Application is NOT running (stale PID file)"; \
			echo "   Cleaning up..."; \
			rm -f $(PID_FILE); \
		fi \
	else \
		echo "❌ Application is NOT running (no PID file)"; \
		echo ""; \
		echo "💡 Start with: make start  (background) or  make run  (interactive)"; \
	fi
	@echo ""

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
	@echo "🧹 Cleaning up..."
	@$(MAKE) stop 2>/dev/null || true
	@docker-compose down -v
	@rm -f $(PID_FILE) $(LOG_FILE)
	@cargo clean
	@echo "✅ Cleanup complete"

# Quick Start Guide

This guide shows you how to quickly get the ScyllaDB CDC Event Sourcing application up and running.

## Prerequisites

- Docker and Docker Compose
- Rust (latest stable)
- `make` utility

## Quick Start (Background Mode)

**Start everything in background:**
```bash
make start
```

This will:
1. Start ScyllaDB and Redpanda containers
2. Initialize the database schema
3. Run the event sourcing demo
4. Keep the application running in the background

**Expected output:**
```
✅ Application started successfully!

📊 Available endpoints:
   Metrics:      http://localhost:9090/metrics
   Health:       http://localhost:9090/health

💡 Useful commands:
   make logs     - View application logs
   make metrics  - Check metrics
   make status   - Check if running
   make stop     - Stop application
```

**Check application status:**
```bash
make status
```

**View logs in real-time:**
```bash
make logs
```

**Stop the application:**
```bash
make stop
```

## Quick Start (Interactive Mode)

**Run interactively (output to terminal):**
```bash
make dev
```

This starts services and runs the app in your terminal. Press `Ctrl+C` to stop.

## Access Endpoints

Once running, access these URLs in your browser:

### Metrics Endpoint
```
http://localhost:9090/metrics
```

View Prometheus-format metrics including:
- CDC processing latency histograms
- Event store operation latency
- Throughput counters
- Error rates

### Health Check
```
http://localhost:9090/health
```

Returns JSON:
```json
{
  "status": "healthy",
  "service": "scylladb-cdc-outbox"
}
```

## Quick Metrics Check

**View current metrics from terminal:**
```bash
make metrics
```

**Check specific metrics:**
```bash
# CDC processing duration (for p95 calculations)
curl http://localhost:9090/metrics | grep cdc_processing_duration

# Event store append latency
curl http://localhost:9090/metrics | grep event_store_append_duration

# All event-related metrics
curl http://localhost:9090/metrics | grep event_
```

## Available Make Commands

```bash
make help           # Show all available commands

# Quick Start
make start          # Start in background
make stop           # Stop background app
make dev            # Start in interactive mode

# Monitoring
make status         # Check if running
make logs           # Tail logs (background mode)
make metrics        # View current metrics

# Database
make reset          # Clean restart with fresh data
make schema         # Re-initialize schema

# Build & Test
make build          # Build release binary
make test           # Run unit tests

# Cleanup
make clean          # Stop everything and clean up
```

## Common Workflows

### Development Workflow
```bash
# Start fresh environment
make reset

# Run in interactive mode (see output in terminal)
make run

# In another terminal, check metrics
make metrics

# Stop with Ctrl+C
```

### Production-like Workflow
```bash
# Start in background
make start

# Check it's running
make status

# View logs
make logs

# Check metrics
curl http://localhost:9090/metrics

# Stop when done
make stop
```

### After Code Changes
```bash
# If app is running in background, stop it first
make stop

# Rebuild
cargo build

# Start again
make start
```

## Viewing Latency Percentiles

The application exposes histogram metrics that can be used to calculate p50, p95, p99 latencies.

### Using Prometheus Queries

If you have Prometheus running, use these queries:

**p95 CDC Processing Latency:**
```promql
histogram_quantile(0.95,
  rate(cdc_processing_duration_seconds_bucket[5m])
)
```

**p99 Event Store Append Latency:**
```promql
histogram_quantile(0.99,
  rate(event_store_append_duration_seconds_bucket[5m])
)
```

**p50 (median) latency:**
```promql
histogram_quantile(0.50,
  rate(cdc_processing_duration_seconds_bucket[5m])
)
```

See **OBSERVABILITY.md** for complete Prometheus setup and query examples.

### Quick Prometheus Setup

**1. Create `prometheus.yml`:**
```yaml
scrape_configs:
  - job_name: 'scylladb_cdc'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 15s
```

**2. Run Prometheus (using Docker):**
```bash
docker run -d \
  --name prometheus \
  --network host \
  -v $(pwd)/prometheus.yml:/etc/prometheus/prometheus.yml \
  prom/prometheus
```

**3. Access Prometheus UI:**
```
http://localhost:9090
```

## Troubleshooting

### Application won't start

**Check if services are running:**
```bash
docker-compose ps
```

**Check logs:**
```bash
# If started with 'make start'
make logs

# Or directly
tail -100 app.log
```

**Common issues:**
- ScyllaDB not ready → Wait 30 seconds and try again
- Port 9090 already in use → Check for other processes using the port
- PID file exists → Run `make stop` first

### Can't access metrics endpoint

**Check if app is running:**
```bash
make status
```

**Test health endpoint:**
```bash
curl http://localhost:9090/health
```

**Check if port is listening:**
```bash
lsof -i :9090
# or
netstat -an | grep 9090
```

### Clean slate restart

If something is stuck, do a complete cleanup:
```bash
make clean
make start
```

## Next Steps

- **Explore Observability**: See [OBSERVABILITY.md](OBSERVABILITY.md) for detailed metrics documentation
- **Set up Grafana**: Create dashboards for latency percentiles
- **Configure Alerts**: Set up Prometheus alerts for high latency or errors
- **Load Testing**: Generate more events to see metrics under load

## Demo Walkthrough

When the application starts, it runs an automated demo that:

1. **Creates an Order** (OrderCreated event)
2. **Confirms the Order** (OrderConfirmed event)
3. **Ships the Order** (OrderShipped event with tracking)
4. **Delivers the Order** (OrderDelivered event)
5. **Registers a Customer** (CustomerRegistered event)
6. **Adds Customer Address** (AddressAdded event)
7. **Upgrades Customer Tier** (TierUpgraded event)

All these events flow through:
- Event Store (with optimistic locking)
- Outbox table (atomic write with events)
- CDC Stream (ScyllaDB CDC)
- Redpanda (event bus)

Each operation is instrumented with metrics for latency tracking.

## What Gets Measured

### Latency Histograms (for p50/p95/p99)
- CDC event processing duration
- Event store append operations
- Event store load operations

### Counters
- Events processed successfully
- Events failed
- Retry attempts
- DLQ messages

### Gauges
- Circuit breaker state
- Actor health status

All metrics are available at `http://localhost:9090/metrics` in Prometheus format.

---

For detailed observability setup, see **[OBSERVABILITY.md](OBSERVABILITY.md)**

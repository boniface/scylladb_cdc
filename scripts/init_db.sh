#!/bin/bash

# Database initialization script for ScyllaDB CDC Outbox Pattern Demo
# This script sets up all required tables for the application

set -e

SCYLLA_HOST="${SCYLLA_HOST:-127.0.0.1}"
SCYLLA_PORT="${SCYLLA_PORT:-9042}"

echo "🔧 Initializing ScyllaDB schema..."
echo "   Host: $SCYLLA_HOST:$SCYLLA_PORT"

# Wait for ScyllaDB to be ready
echo "⏳ Waiting for ScyllaDB to be ready..."
for i in {1..30}; do
    if cqlsh $SCYLLA_HOST $SCYLLA_PORT -e "SELECT now() FROM system.local;" &>/dev/null; then
        echo "✅ ScyllaDB is ready!"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "❌ ScyllaDB did not become ready in time"
        exit 1
    fi
    sleep 2
done

# Execute the schema
echo "📝 Creating keyspace and tables..."
cqlsh $SCYLLA_HOST $SCYLLA_PORT < src/db/schema.cql

echo "✅ Database initialization complete!"
echo ""
echo "📊 Verifying tables..."
cqlsh $SCYLLA_HOST $SCYLLA_PORT -e "
USE orders_ks;
DESCRIBE TABLES;
"

echo ""
echo "🎉 Setup complete! You can now run the application."

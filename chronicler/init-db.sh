#!/bin/bash
set -e

# Wait for ClickHouse to be ready
until clickhouse-client --host localhost --query "SELECT 1"; do
    echo "Waiting for ClickHouse to be ready..."
    sleep 1
done

# Create the database if it doesn't exist
clickhouse-client --query "CREATE DATABASE IF NOT EXISTS $CLICKHOUSE_DB"

# Run all SQL files in order
for f in /docker-entrypoint-initdb.d/*.sql; do
    case "$f" in
        *.sql)    echo "$0: running $f"; clickhouse-client --database=$CLICKHOUSE_DB --multiquery < "$f"; echo ;;
        *)        echo "$0: ignoring $f" ;;
    esac
done

echo "Database initialization completed" 
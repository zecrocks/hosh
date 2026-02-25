FROM clickhouse/clickhouse-server:25.1-alpine

# Disable system log tables to prevent disk bloat
COPY config.d/disable-system-logs.xml /etc/clickhouse-server/config.d/

# Copy migrations to a directory in the container
COPY migrations/ /docker-entrypoint-initdb.d/migrations/

# Add custom initialization script that runs migrations
COPY init-db.sh /docker-entrypoint-initdb.d/

# Make the initialization script executable
RUN chmod +x /docker-entrypoint-initdb.d/init-db.sh 
-- Initial schema for Hosh ClickHouse database
-- Creates the core tables and materialized views

CREATE DATABASE IF NOT EXISTS hosh;

-- Results table - stores individual check results
CREATE TABLE IF NOT EXISTS hosh.results (
    target_id UUID,
    checked_at DateTime64(3, 'UTC'),
    hostname String,
    resolved_ip String,
    ip_version UInt8,
    checker_module String,
    status String,
    ping_ms Float32,
    checker_location String,
    checker_id UUID,
    response_data String,
    user_submitted Boolean DEFAULT false
) ENGINE = MergeTree()
ORDER BY (hostname, checker_module, checked_at)
PARTITION BY toYYYYMM(checked_at);

-- Targets table - stores the list of targets to be checked
CREATE TABLE IF NOT EXISTS hosh.targets (
    target_id UUID,
    module String,
    hostname String,
    last_queued_at DateTime64(3, 'UTC'),
    last_checked_at DateTime64(3, 'UTC'),
    user_submitted Boolean DEFAULT false
) ENGINE = MergeTree()
ORDER BY (hostname, module);

-- Uptime statistics materialized view
CREATE MATERIALIZED VIEW IF NOT EXISTS hosh.uptime_stats
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(time_bucket)
ORDER BY (hostname, time_bucket)
AS SELECT
    hostname,
    toStartOfHour(checked_at) AS time_bucket,
    countIf(status = 'online') AS online_count,
    count() AS total_checks
FROM hosh.results
GROUP BY hostname, time_bucket;

-- Create indices for better query performance
ALTER TABLE hosh.results ADD INDEX IF NOT EXISTS idx_hostname_status (hostname, status) TYPE minmax GRANULARITY 1;
ALTER TABLE hosh.results ADD INDEX IF NOT EXISTS idx_checked_at (checked_at) TYPE minmax GRANULARITY 1;
ALTER TABLE hosh.targets ADD INDEX IF NOT EXISTS idx_hostname (hostname) TYPE minmax GRANULARITY 1; 
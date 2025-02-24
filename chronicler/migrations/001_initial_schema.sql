CREATE TABLE IF NOT EXISTS results
(
    target_id UUID,
    checked_at DateTime64(3, 'UTC'),
    hostname String,
    resolved_ip String,
    ip_version UInt8,
    checker_module LowCardinality(String),
    status LowCardinality(String),
    ping_ms Float32,
    checker_location LowCardinality(String),
    checker_id UUID,
    response_data String
)
ENGINE = MergeTree()
ORDER BY (checked_at, target_id);

CREATE TABLE IF NOT EXISTS targets
(
    target_id UUID,
    module LowCardinality(String),
    hostname String,
    last_queued_at DateTime64(3, 'UTC'),
    last_checked_at DateTime64(3, 'UTC'),
    user_submitted Boolean
)
ENGINE = MergeTree()
ORDER BY (hostname, target_id);

-- Create materialized view for uptime statistics
CREATE MATERIALIZED VIEW IF NOT EXISTS uptime_stats
ENGINE = AggregatingMergeTree()
ORDER BY (hostname, time_bucket)
AS SELECT
    hostname,
    toStartOfHour(checked_at) as time_bucket,
    countIf(status = 'online') as online_count,
    count(*) as total_checks
FROM results
GROUP BY hostname, time_bucket; 
-- Add port-aware uptime statistics materialized view
-- This creates a new view that groups by both hostname and port

CREATE MATERIALIZED VIEW IF NOT EXISTS hosh.uptime_stats_by_port
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(time_bucket)
ORDER BY (hostname, port, time_bucket)
AS SELECT
    hostname,
    JSONExtractString(response_data, 'port') as port,
    toStartOfHour(checked_at) AS time_bucket,
    countIf(status = 'online') AS online_count,
    count() AS total_checks
FROM hosh.results
WHERE response_data != ''
GROUP BY hostname, port, time_bucket;

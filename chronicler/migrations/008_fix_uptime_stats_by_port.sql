-- Fix the uptime_stats_by_port materialized view
--
-- This migration recreates the MV to use the port column directly (added in 007)
-- instead of extracting from response_data JSON which gets TTL'd.

-- ============================================================================
-- STEP 1: Drop the old materialized view and its underlying table
-- ============================================================================

DROP VIEW IF EXISTS hosh.uptime_stats_by_port;
DROP TABLE IF EXISTS hosh.`.inner.uptime_stats_by_port`;
DROP TABLE IF EXISTS hosh.`.inner_id.2981abcf-a3b1-4411-ac3a-04a1feda7012`;
DROP TABLE IF EXISTS hosh.`.inner_id.3fcde282-e774-4c43-8781-a6fb50534b5e`;

-- ============================================================================
-- STEP 2: Recreate the materialized view using the port column
-- ============================================================================

CREATE MATERIALIZED VIEW hosh.uptime_stats_by_port
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(time_bucket)
ORDER BY (hostname, port, time_bucket)
AS SELECT
    hostname,
    toString(port) as port,
    toStartOfHour(checked_at) AS time_bucket,
    countIf(status = 'online') AS online_count,
    count() AS total_checks
FROM hosh.results
GROUP BY hostname, port, time_bucket;

-- ============================================================================
-- STEP 3: Backfill with historical data (only if results table has data)
-- ============================================================================
-- On fresh install, this is a no-op since results table is empty.
-- On existing install, this populates the view with historical data.

INSERT INTO hosh.uptime_stats_by_port
SELECT
    hostname,
    toString(r.port) as port,
    toStartOfHour(checked_at) AS time_bucket,
    countIf(status = 'online') AS online_count,
    count() AS total_checks
FROM hosh.results r
WHERE r.port > 0
GROUP BY hostname, r.port, time_bucket;

-- Fix the uptime_stats_by_port materialized view and backfill port column
--
-- Problems with the original view:
-- 1. Uses JSONExtractString(response_data, 'port') which won't work after TTL clears response_data
-- 2. Has WHERE response_data != '' which filters out rows after TTL clears the JSON
-- 3. Port type mismatch: MV has String port, results table now has UInt16 port column
--
-- Solution:
-- 1. Backfill the port column in results table from targets table (for rows where JSON was TTL'd)
-- 2. Drop and recreate the MV to use the port column directly
-- 3. Backfill historical data into the new MV
-- 4. Use toString(port) to maintain compatibility with existing query JOINs

-- ============================================================================
-- STEP 1: Backfill port column in results table from targets table
-- ============================================================================
-- This fixes rows where port=0 because response_data was already cleared by TTL
-- and the original backfill in migration 007 couldn't extract the port from JSON.

-- Create a temporary mapping table from targets
CREATE TABLE IF NOT EXISTS hosh.port_mapping ENGINE = Memory AS
SELECT DISTINCT hostname, module, port 
FROM hosh.targets 
WHERE port > 0;

-- Create a dictionary for efficient lookups during UPDATE
CREATE OR REPLACE DICTIONARY hosh.port_dict (
    hostname String,
    module String,
    port UInt16
)
PRIMARY KEY hostname, module
SOURCE(CLICKHOUSE(QUERY 'SELECT hostname, module, port FROM hosh.port_mapping'))
LAYOUT(COMPLEX_KEY_HASHED())
LIFETIME(0);

-- Backfill port column using the dictionary
-- This updates all rows where port=0 with the correct port from targets
ALTER TABLE hosh.results
UPDATE port = dictGet('hosh.port_dict', 'port', (hostname, checker_module))
WHERE port = 0 
AND dictHas('hosh.port_dict', (hostname, checker_module));

-- Wait for mutation to complete before proceeding
-- Check with: SELECT * FROM system.mutations WHERE table = 'results' AND is_done = 0;

-- Cleanup temporary objects
DROP DICTIONARY IF EXISTS hosh.port_dict;
DROP TABLE IF EXISTS hosh.port_mapping;

-- ============================================================================
-- STEP 2: Recreate the materialized view
-- ============================================================================

-- Drop the old materialized view
DROP VIEW IF EXISTS hosh.uptime_stats_by_port;

-- Drop the underlying table (MV creates a hidden table)
DROP TABLE IF EXISTS hosh.`.inner.uptime_stats_by_port`;

-- Recreate the materialized view using the port column instead of JSON extraction
-- This will work correctly even after TTL clears response_data
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
-- STEP 3: Backfill the materialized view with historical data
-- ============================================================================

-- This populates the view with all existing data from the results table
INSERT INTO hosh.uptime_stats_by_port
SELECT
    hostname,
    toString(port) as port,
    toStartOfHour(checked_at) AS time_bucket,
    countIf(status = 'online') AS online_count,
    count() AS total_checks
FROM hosh.results
WHERE port > 0
GROUP BY hostname, port, time_bucket;

-- ============================================================================
-- Verification queries (run manually after migration)
-- ============================================================================

-- Verify port backfill worked (should show few/no port=0 rows):
-- SELECT port, count() as row_count FROM hosh.results 
-- WHERE checked_at >= now() - INTERVAL 7 DAY GROUP BY port ORDER BY row_count DESC LIMIT 10;

-- Verify MV has data:
-- SELECT count(), min(time_bucket), max(time_bucket) FROM hosh.uptime_stats_by_port;

-- Verify no duplicate servers per hostname (should return 0 rows):
-- SELECT hostname, groupArray(DISTINCT port) as ports, count(DISTINCT port) as port_count
-- FROM hosh.results WHERE checker_module = 'zec' AND checked_at >= now() - INTERVAL 7 DAY AND port > 0
-- GROUP BY hostname HAVING port_count > 1 LIMIT 20;

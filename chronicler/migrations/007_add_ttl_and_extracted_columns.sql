-- Add extracted columns for data we want to keep forever,
-- and add TTL to response_data to clear it after 7 days.
--
-- This reduces disk usage while preserving uptime history and key metadata.
-- 
-- Data retention strategy:
-- - Forever: hostname, port, status, ping_ms, server_version, error, block_height,
--            resolved_ip, checker_location, checked_at, checker_module
-- - 7 days: response_data (full JSON with all details)
--
-- The uptime_stats_by_port materialized view aggregates at insert time,
-- so uptime calculations remain accurate even after response_data is cleared.

-- Add server_version column (extracted from response_data)
ALTER TABLE hosh.results
ADD COLUMN IF NOT EXISTS server_version String DEFAULT '';

-- Add error column (extracted from response_data)
ALTER TABLE hosh.results
ADD COLUMN IF NOT EXISTS error String DEFAULT '';

-- Add block_height column (extracted from response_data, useful for historical queries)
ALTER TABLE hosh.results
ADD COLUMN IF NOT EXISTS block_height UInt64 DEFAULT 0;

-- Add port column to results table (currently only in response_data JSON)
ALTER TABLE hosh.results
ADD COLUMN IF NOT EXISTS port UInt16 DEFAULT 0;

-- Backfill the new columns from existing response_data JSON
-- Run these BEFORE adding TTL so we don't lose the data

-- Backfill server_version
ALTER TABLE hosh.results
UPDATE server_version = JSONExtractString(response_data, 'server_version')
WHERE response_data != '' AND server_version = '';

-- Backfill error
ALTER TABLE hosh.results
UPDATE error = JSONExtractString(response_data, 'error')
WHERE response_data != '' AND error = '';

-- Backfill block_height
ALTER TABLE hosh.results
UPDATE block_height = JSONExtractUInt(response_data, 'height')
WHERE response_data != '' AND block_height = 0;

-- Backfill port
ALTER TABLE hosh.results
UPDATE port = toUInt16OrZero(JSONExtractString(response_data, 'port'))
WHERE response_data != '' AND port = 0;

-- Wait for mutations to complete before adding TTL
-- You can check progress with: SELECT * FROM system.mutations WHERE is_done = 0;

-- Add column-level TTL to response_data: clear to empty string after 7 days
-- This keeps the row but removes the bulky JSON payload
-- Note: checked_at is DateTime64, must cast to DateTime for TTL
ALTER TABLE hosh.results
MODIFY COLUMN response_data String DEFAULT '' TTL toDateTime(checked_at) + INTERVAL 7 DAY;

-- Note: resolved_ip and checker_location columns already exist in the schema
-- and will persist forever. The web code has been updated to populate the new columns.


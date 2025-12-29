-- Migration 004: System log cleanup (no-op)
--
-- Originally this migration set TTL on ClickHouse system log tables (query_log,
-- trace_log, etc.) to limit disk usage. However, these tables don't exist in the
-- Alpine-based ClickHouse image used by this project.
--
-- If you switch to a full ClickHouse image and want to limit system log retention,
-- you can configure it in ClickHouse's config.xml or users.xml.
--
-- See: https://clickhouse.com/docs/en/operations/system-tables

SELECT 1;

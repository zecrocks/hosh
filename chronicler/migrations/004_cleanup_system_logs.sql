-- Migration 004: System log cleanup (no-op)
--
-- Originally this migration set TTL on ClickHouse system log tables.
-- System logs are now fully disabled via config.d/disable-system-logs.xml
-- and existing data is cleaned up in migration 011.

SELECT 1;

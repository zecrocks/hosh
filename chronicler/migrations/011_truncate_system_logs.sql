-- Migration 012: Truncate existing system log data
--
-- The config.d/disable-system-logs.xml config disables all system log tables
-- going forward. This migration cleans up any existing data that accumulated
-- before the config was applied.

TRUNCATE TABLE IF EXISTS system.trace_log;
TRUNCATE TABLE IF EXISTS system.text_log;
TRUNCATE TABLE IF EXISTS system.query_log;
TRUNCATE TABLE IF EXISTS system.query_thread_log;
TRUNCATE TABLE IF EXISTS system.query_views_log;
TRUNCATE TABLE IF EXISTS system.metric_log;
TRUNCATE TABLE IF EXISTS system.asynchronous_metric_log;
TRUNCATE TABLE IF EXISTS system.part_log;
TRUNCATE TABLE IF EXISTS system.processors_profile_log;
TRUNCATE TABLE IF EXISTS system.query_metric_log;
TRUNCATE TABLE IF EXISTS system.error_log;

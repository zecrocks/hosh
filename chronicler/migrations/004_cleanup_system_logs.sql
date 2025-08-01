-- Clean up ClickHouse system logs with TTL settings
-- System logs: keep 1–3 days depending on usefulness

ALTER TABLE system.trace_log
  MODIFY TTL event_time + INTERVAL 1 DAY;

ALTER TABLE system.text_log
  MODIFY TTL event_time + INTERVAL 1 DAY;

ALTER TABLE system.query_log
  MODIFY TTL event_time + INTERVAL 3 DAY;

ALTER TABLE system.processors_profile_log
  MODIFY TTL event_time + INTERVAL 1 DAY;

ALTER TABLE system.metric_log
  MODIFY TTL event_time + INTERVAL 3 DAY;

ALTER TABLE system.asynchronous_metric_log
  MODIFY TTL event_time + INTERVAL 2 DAY;

ALTER TABLE system.query_views_log
  MODIFY TTL event_time + INTERVAL 3 DAY;

ALTER TABLE system.query_metric_log
  MODIFY TTL event_time + INTERVAL 3 DAY;

ALTER TABLE system.part_log
  MODIFY TTL event_time + INTERVAL 3 DAY;

ALTER TABLE system.error_log
  MODIFY TTL event_time + INTERVAL 3 DAY; 
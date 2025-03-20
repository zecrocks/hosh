# ClickHouse Schema
# ClickHouse Schema and Integration

This page documents the ClickHouse database schema used by Hosh and how different services interact with it.

## Schema Overview

The ClickHouse database consists of two main tables and a materialized view:

### Results Table

Stores individual check results from all checkers:

| Column | Type | Description |
|--------|------|-------------|
| target_id | UUID | Unique identifier for the target |
| checked_at | DateTime64(3, 'UTC') | Timestamp of the check |
| hostname | String | Target hostname (.onion for Tor) |
| resolved_ip | String | IP address that was checked |
| ip_version | UInt8 | IP version (4 or 6) |
| checker_module | String | Module that performed check (http, zec, btc) |
| status | String | 'online' or 'offline' |
| ping_ms | Float32 | Response time in milliseconds |
| checker_location | String | Geographic location of checker |
| checker_id | UUID | Unique ID of the checker instance |
| response_data | String | Full check response data |
| user_submitted | Boolean | Whether target was submitted by a user |

The Results table uses the MergeTree engine with:
- Partitioning by month: `PARTITION BY toYYYYMM(checked_at)`
- Ordering by: `ORDER BY (hostname, checker_module, checked_at)`
- Indices on: `(hostname, status)` and `checked_at`

### Targets Table

Stores the list of targets to be checked:

| Column | Type | Description |
|--------|------|-------------|
| target_id | UUID | Unique identifier for the target |
| module | String | Checker module to use |
| hostname | String | Target hostname |
| last_queued_at | DateTime64(3, 'UTC') | Last time target was queued |
| last_checked_at | DateTime64(3, 'UTC') | Last time target was checked |
| user_submitted | Boolean | Whether target was submitted by a user |

The Targets table uses the MergeTree engine with:
- Ordering by: `ORDER BY (hostname, module)`
- Index on: `hostname`

### Uptime Statistics View

The `uptime_stats` materialized view maintains aggregated uptime statistics:

| Column | Type | Description |
|--------|------|-------------|
| hostname | String | Target hostname |
| time_bucket | DateTime64 | Start of hour for this aggregate |
| online_count | UInt64 | Number of successful checks in this hour |
| total_checks | UInt64 | Total number of checks in this hour |

## Service Integration

### Publisher Service
- Queries the Targets table to determine which hosts need checking
- Updates `last_queued_at` timestamps
- Uses the Results table to avoid duplicate checks

### Checker Services (BTC, HTTP, etc.)
- Write check results to the Results table
- Include their unique checker_id and location
- Store detailed response data as JSON

### Discovery Service
- Inserts new targets into the Targets table
- Updates target information as new nodes are discovered
- Marks targets as user_submitted when appropriate

### Dashboard Service
- Queries the uptime_stats view for efficient uptime calculations
- Reads from Results table for detailed check history
- Uses Targets table for current target status

## Common Queries

### Recent Check Results


```sql
SELECT * FROM results
ORDER BY checked_at DESC
LIMIT 100
```

### Uptime Statistics

```sql
SELECT * FROM uptime_stats
ORDER BY hostname, time_bucket DESC
LIMIT 100
``` 

### Check Triggers

```sql
SELECT * FROM check_triggers
ORDER BY hostname, time_bucket DESC
LIMIT 100
```

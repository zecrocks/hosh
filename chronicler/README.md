# Chronicler Service

Chronicler is Hosh's storage backend service, responsible for storing historical check results and target information. It uses ClickHouse as its default database implementation, though the architecture supports other storage backends.

## Architecture

Chronicler is designed with a pluggable storage backend architecture. While ClickHouse is the default implementation, the system is designed to allow other storage backends (PostgreSQL, BigQuery, etc.) to be implemented in the future.

### Schema

#### Results Table

Stores individual check results:

| Column | Type | Description |
|--------|------|-------------|
| target_id | UUID | Unique identifier for the target |
| checked_at | DateTime64 | Timestamp of the check (with timezone) |
| hostname | String | Target hostname (.onion for Tor) |
| resolved_ip | String | IP address that was checked |
| ip_version | UInt8 | IP version (4 or 6) |
| checker_module | String | Module that performed check (http, zec, btc) |
| status | String | 'online' or 'offline' |
| ping_ms | Float32 | Response time in milliseconds |
| checker_location | String | Geographic location of checker (e.g., country code) |
| checker_id | UUID | Unique ID of the checker instance |
| response_data | JSON | Full check response data |

#### Targets Table

Stores the list of targets to be checked:

| Column | Type | Description |
|--------|------|-------------|
| target_id | UUID | Unique identifier for the target |
| module | String | Checker module to use |
| hostname | String | Target hostname |
| last_queued_at | DateTime64 | Last time target was queued for checking |
| last_checked_at | DateTime64 | Last time target was checked |
| user_submitted | Boolean | Whether target was submitted by a user |

#### Materialized Views

##### Uptime Statistics
The `uptime_stats` materialized view maintains aggregated uptime statistics by hostname and hour:

| Column | Type | Description |
|--------|------|-------------|
| hostname | String | Target hostname |
| time_bucket | DateTime64 | Start of hour for this aggregate |
| online_count | UInt64 | Number of successful checks in this hour |
| total_checks | UInt64 | Total number of checks in this hour |

## Usage

The service is deployed via Docker Compose and requires the following environment variables:

- `CLICKHOUSE_DB`: Database name (default: hosh)
- `CLICKHOUSE_USER`: Database user (default: hosh)
- `CLICKHOUSE_PASSWORD`: Database password

## Migrations

The service uses numbered migration files in the `migrations/` directory to manage database schema. These migrations run automatically when the service starts:

```
chronicler/
├── migrations/
│   └── 001_initial_schema.sql    # Creates base tables and views
└── README.md
```

To add new schema changes:
1. Create a new numbered migration file (e.g., `002_add_new_feature.sql`)
2. Add the SQL commands for your schema changes
3. The changes will be applied on next service startup

## Integration Points

- **Publisher**: Queries the Targets table to determine which hosts need checking
- **Discovery**: Inserts new targets into the Targets table
- **Checkers**: Writes results to the Results table via the storage backend interface

## Example Queries

### 30-Day Uptime Percentage

```sql
-- Using the base results table
SELECT 
    hostname,
    countIf(status = 'online') * 100.0 / count(*) as uptime_percentage
FROM results
WHERE checked_at >= now() - INTERVAL 30 DAY
GROUP BY hostname;

-- Using the materialized view (more efficient)
SELECT 
    hostname,
    sum(online_count) * 100.0 / sum(total_checks) as uptime_percentage
FROM uptime_stats
WHERE time_bucket >= now() - INTERVAL 30 DAY
GROUP BY hostname;
```

### Hourly Uptime Trend

```sql
SELECT 
    hostname,
    time_bucket,
    online_count * 100.0 / total_checks as uptime_percentage
FROM uptime_stats
WHERE hostname = 'example.onion'
  AND time_bucket >= now() - INTERVAL 24 HOUR
ORDER BY time_bucket;
```

## Future Development

- Implementation of additional storage backends
- Complete migration from Redis to structured storage
- Enhanced query interfaces for uptime statistics
- Geographic distribution of check results

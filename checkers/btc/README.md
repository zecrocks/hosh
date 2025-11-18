# Bitcoin Backend Service

A dual-mode service that provides both an Electrum server API and a worker for checking Electrum server health.

## Modes of Operation

The service can run in two modes, controlled by the `RUN_MODE` environment variable:

### API Server Mode (default)
Provides HTTP endpoints for querying Electrum servers:

- `GET /` - API information
- `GET /healthz` - Health check endpoint
- `GET /electrum/servers` - List available Electrum servers
- `GET /electrum/query` - Query specific Electrum server
- `GET /electrum/peers` - Get peer information

### Worker Mode
Polls the web service for Electrum servers to check:

- Polls `GET /api/v1/jobs` endpoint every 10 seconds
- Checks servers every 5 minutes
- Processes check requests concurrently (default: 3 concurrent)
- Submits results to `POST /api/v1/results` endpoint

## Configuration

Environment variables:
```env
# Service Mode
RUN_MODE=worker|server  # defaults to server if not set

# Worker Configuration
WEB_API_URL=http://web:8080
API_KEY=your_api_key_here
MAX_CONCURRENT_CHECKS=3  # default

# ClickHouse Configuration
CLICKHOUSE_HOST=chronicler
CLICKHOUSE_PORT=8123
CLICKHOUSE_DB=hosh
CLICKHOUSE_USER=hosh
CLICKHOUSE_PASSWORD=your_password

# Tor Configuration
TOR_PROXY_HOST=tor
TOR_PROXY_PORT=9050

# Logging
RUST_LOG=info
```

## Development

The service can be run in development mode using:

```bash
docker compose --profile dev up checker-btc-dev
```

This mounts the source code directory and uses cargo for live reloading.

## Architecture

### Core Components

- `main.rs` - Entry point with mode selection
- `routes/` - API endpoint handlers
  - `api_info.rs` - Root endpoint
  - `health.rs` - Health check
  - `electrum.rs` - Electrum server interactions
- `worker/` - Worker mode implementation
- `utils/` - Shared utilities

### Dependencies

- `axum` - Web framework
- `tokio` - Async runtime
- `reqwest` - HTTP client for web API
- `clickhouse` - ClickHouse database client
- `electrum-client` - Electrum protocol
- `serde` - Serialization
- `tracing` - Logging

## Error Handling

The service implements comprehensive error handling:
- API errors return appropriate HTTP status codes
- Worker mode logs errors and continues processing
- All errors are properly logged with context

## Integration Points

- Web service HTTP API for job distribution and result submission
- ClickHouse for storing check results
- Tor for accessing .onion addresses
- HTTP API for external Electrum queries (API server mode)

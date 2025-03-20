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
Subscribes to NATS messages and performs Electrum server health checks:

- Listens on `hosh.check.btc` subject (configurable)
- Processes check requests concurrently
- Stores results in Redis
- Supports both regular and user-submitted checks

## Configuration

Environment variables:
```env
# Service Mode
RUN_MODE=worker|server  # defaults to server if not set

# Worker Configuration
NATS_URL=nats://nats:4222
NATS_SUBJECT=hosh.check.btc
MAX_CONCURRENT_CHECKS=3  # default

# Redis Configuration
REDIS_HOST=redis
REDIS_PORT=6379

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
- `async-nats` - NATS client
- `redis` - Redis client
- `electrum-client` - Electrum protocol
- `serde` - Serialization
- `tracing` - Logging

## Error Handling

The service implements comprehensive error handling:
- API errors return appropriate HTTP status codes
- Worker mode logs errors and continues processing
- All errors are properly logged with context

## Integration Points

- NATS for receiving check requests
- Redis for storing check results
- Tor for accessing .onion addresses
- HTTP API for external queries

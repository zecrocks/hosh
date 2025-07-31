# ZEC Checker

A Rust-based service that monitors Zcash (ZEC) lightwalletd servers by connecting to their gRPC endpoints and checking their health status.

## Overview

The ZEC checker connects to Zcash lightwalletd servers using the gRPC protocol to verify their health and collect detailed server information. It subscribes to NATS messages and performs health checks on ZEC servers, storing results in ClickHouse.

## Features

- **gRPC Connection**: Connects to Zcash lightwalletd servers using TLS
- **Server Information**: Collects detailed server metadata including:
  - Block height and estimated height
  - Vendor information and git commit
  - Chain name and consensus branch ID
  - Sapling activation height
  - Server version and build information
  - Donation address
- **Health Monitoring**: Tracks server status and response times
- **ClickHouse Integration**: Stores check results with detailed metadata
- **NATS Integration**: Receives check requests via message queue

## How It Works

1. **NATS Subscription**: Listens for check requests on NATS topic
2. **gRPC Connection**: Establishes TLS connection to lightwalletd server
3. **Server Query**: Requests server information using `GetLatestBlock` RPC
4. **Data Collection**: Gathers comprehensive server metadata
5. **Result Storage**: Stores results in ClickHouse with full context

## Server Information Collected

The checker collects the following information from each ZEC server:

| Field | Description |
|-------|-------------|
| `block_height` | Current blockchain height |
| `vendor` | Server vendor information |
| `git_commit` | Git commit hash |
| `chain_name` | Zcash chain name (mainnet/testnet) |
| `sapling_activation_height` | Sapling protocol activation height |
| `consensus_branch_id` | Consensus branch identifier |
| `taddr_support` | Transparent address support |
| `branch` | Git branch name |
| `build_date` | Build timestamp |
| `build_user` | Build user information |
| `estimated_height` | Estimated blockchain height |
| `version` | Server version |
| `zcashd_build` | Zcashd build information |
| `zcashd_subversion` | Zcashd subversion |
| `donation_address` | Server donation address |

## Configuration

Environment variables:

```env
# NATS Configuration
NATS_URL=nats://nats:4222
NATS_SUBJECT=hosh.check.zec

# ClickHouse Configuration
CLICKHOUSE_HOST=chronicler
CLICKHOUSE_PORT=8123
CLICKHOUSE_DB=hosh
CLICKHOUSE_USER=hosh
CLICKHOUSE_PASSWORD=your_password

# HTTP Client Configuration
HTTP_TIMEOUT_SECONDS=30

# Logging
RUST_LOG=info
```

## Data Storage

Results are stored in ClickHouse with the following structure:

### Check Results
- `target_id`: Unique identifier for the target
- `checked_at`: Timestamp of the check
- `hostname`: Server hostname
- `resolved_ip`: IP address that was checked
- `ip_version`: IP version (4 or 6)
- `checker_module`: Set to "zec"
- `status`: "online" or "offline"
- `ping_ms`: Response time in milliseconds
- `checker_location`: Geographic location of checker
- `checker_id`: Unique ID of the checker instance
- `response_data`: Full JSON response with server metadata
- `user_submitted`: Whether target was submitted by a user

## Error Handling

The service implements comprehensive error handling:
- **Connection Errors**: TLS connection failures are logged
- **Timeout Handling**: Configurable timeouts prevent hanging requests
- **Parse Errors**: Invalid server responses are handled gracefully
- **NATS Errors**: Connection issues are logged and retried

## Development

### Local Development
```bash
cd projects/hosh/checkers/zec
cargo run
```

### Docker Development
```bash
docker compose --profile dev up checker-zec-dev
```

### Production Build
```bash
docker build -f Dockerfile -t checker-zec .
```

## Dependencies

- `tokio` - Async runtime
- `zcash_client_backend` - Zcash gRPC client
- `tonic` - gRPC framework
- `async-nats` - NATS client
- `reqwest` - HTTP client
- `serde` - Serialization
- `chrono` - DateTime handling
- `uuid` - Unique identifiers
- `tracing` - Logging

## Integration Points

- **NATS**: Receives check requests and publishes results
- **ClickHouse**: Stores detailed check results and metadata
- **gRPC**: Connects to Zcash lightwalletd servers
- **TLS**: Secure connections to ZEC servers

## Architecture

The checker follows a worker pattern:
1. **Initialization**: Sets up NATS connection and ClickHouse config
2. **Message Processing**: Listens for check requests
3. **Server Connection**: Establishes gRPC connection to target server
4. **Data Collection**: Gathers server information and metadata
5. **Result Storage**: Stores comprehensive results in ClickHouse

## Monitoring

The service provides detailed logging for monitoring:
- Connection attempts and results
- Server response times
- Error conditions and recovery
- ClickHouse storage operations

## Security

- **TLS Connections**: All gRPC connections use TLS encryption
- **No Credentials**: No authentication required for read-only operations
- **Timeout Protection**: Prevents hanging connections
- **Error Isolation**: Individual server failures don't affect others 
# ZEC Checker

A Rust-based service that monitors Zcash (ZEC) lightwalletd servers by connecting to their gRPC endpoints and checking their health status.

## Overview

The ZEC checker connects to Zcash lightwalletd servers using the gRPC protocol to verify their health and collect detailed server information. It subscribes to NATS messages and performs health checks on ZEC servers, storing results in ClickHouse.

## Features

- **gRPC Connection**: Connects to Zcash lightwalletd servers using TLS
- **SOCKS5 Proxy Support**: Route connections through SOCKS proxies (Tor, etc.) for privacy or .onion access
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

# SOCKS Proxy Configuration (Optional)
SOCKS_PROXY=127.0.0.1:9050    # Use SOCKS5 proxy (e.g., Tor)
# SOCKS_PROXY=127.0.0.1:9150  # For Tor Browser

# Logging
RUST_LOG=info
```

## SOCKS Proxy Support

The ZEC checker can route connections through a SOCKS5 proxy (e.g., Tor) to access `.onion` hidden services.

### How It Works

The checker **automatically detects** `.onion` addresses and routes them through the configured SOCKS proxy. Regular clearnet servers use direct connections for optimal performance.

**Behavior:**
- `.onion` addresses → **SOCKS proxy** (required)
- Regular servers → **Direct connection** (even if SOCKS_PROXY is set)

### Features

- **Automatic .onion Detection**: Automatically uses SOCKS only for `.onion` addresses
- **Remote DNS Resolution**: DNS queries are handled by the SOCKS proxy, not locally (critical for .onion addresses)
- **Tor Support**: Full support for Tor hidden services
- **Optimal Performance**: Regular servers use fast direct connections
- **TLS Over SOCKS**: Full TLS encryption is maintained for clearnet servers accessed via SOCKS (if needed)

### Configuration

Set the `SOCKS_PROXY` environment variable to enable `.onion` address support:

```bash
# Use Tor (standard port)
export SOCKS_PROXY=127.0.0.1:9050

# Use Tor Browser
export SOCKS_PROXY=127.0.0.1:9150

# Use custom SOCKS proxy
export SOCKS_PROXY=proxy.example.com:1080
```

**Note:** The SOCKS proxy is **only used for `.onion` addresses**. Regular servers will use direct connections regardless of whether `SOCKS_PROXY` is set.

### Testing with Tor

#### Prerequisites

1. **Install Tor**:
   ```bash
   # macOS
   brew install tor
   
   # Ubuntu/Debian
   sudo apt install tor
   ```

2. **Start Tor**:
   ```bash
   # macOS/Linux
   tor
   
   # Or use system service
   sudo systemctl start tor
   ```

3. **Or use Tor Browser** (default port 9150)

#### Test with Public Server

```bash
# Direct connection
cargo run -- --test zec.rocks:443

# Still uses direct connection (SOCKS only for .onion)
SOCKS_PROXY=127.0.0.1:9050 cargo run -- --test zec.rocks:443
```

#### Test with .onion Address

```bash
# .onion addresses require SOCKS proxy
SOCKS_PROXY=127.0.0.1:9050 cargo run -- --test <onion-address>:443
```

### Performance Considerations

**Regular Servers (Direct Connection):**
- Connection time: ~100-500ms (TLS handshake)
- Throughput: Full network speed
- **Always uses direct connection** for optimal performance

**.onion Addresses (Via SOCKS/Tor):**
- Connection time: ~2-5 seconds (Tor circuit establishment)
- Throughput: Reduced (Tor bandwidth limits)
- Additional latency: +200-500ms per request
- **Automatically routed through SOCKS** when detected

### Security Features

1. **DNS Privacy**: DNS resolution happens remotely at the SOCKS proxy, preventing local DNS leaks
2. **TLS Validation**: Certificate validation is still performed, protecting against MITM attacks
3. **Fail-Hard**: If `SOCKS_PROXY` is configured but unreachable, the connection fails rather than falling back to direct connection
4. **Onion Support**: Can connect to `.onion` addresses for enhanced privacy

### Troubleshooting

**Issue**: "Cannot connect to .onion address without SOCKS proxy"
- **Solution**: Set the `SOCKS_PROXY` environment variable to a running Tor instance

**Issue**: "SOCKS connection failed: Proxy server unreachable"
- **Cause**: SOCKS proxy not running
- **Solution**: Start Tor or your SOCKS proxy
- **Check**: `curl --socks5 127.0.0.1:9050 https://check.torproject.org`

**Issue**: Regular server seems slow
- **Cause**: This shouldn't happen - regular servers bypass SOCKS
- **Check**: Verify the server doesn't have `.onion` in its hostname
- **Solution**: Regular servers always use fast direct connections

**Issue**: Very slow connections to .onion addresses
- **Cause**: Tor circuit building / bandwidth limitations
- **Solution**: Normal for Tor (2-5 seconds is expected). Timeouts are automatically increased.

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

### Docker with Tor
The production docker-compose setup automatically configures the ZEC checker to use Tor:

```bash
# Start all services including Tor
docker compose up -d

# The ZEC checkers will automatically use tor:9050 for SOCKS proxy
# This enables .onion address support and anonymous connections
```

The `SOCKS_PROXY=tor:9050` environment variable is pre-configured in `docker-compose.yml`.

## Dependencies

- `tokio` - Async runtime
- `zcash_client_backend` - Zcash gRPC client
- `tonic` - gRPC framework
- `tower` - Service abstraction for custom connectors
- `tokio-socks` - SOCKS5 protocol implementation
- `hyper-util` - Hyper utilities for connection handling
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
- **SOCKS Privacy**: Optional SOCKS proxy support for anonymous connections
- **Remote DNS**: DNS resolution via proxy prevents DNS leaks
- **Certificate Validation**: TLS certificates are validated even over SOCKS
- **No Credentials**: No authentication required for read-only operations
- **Timeout Protection**: Prevents hanging connections
- **Error Isolation**: Individual server failures don't affect others 
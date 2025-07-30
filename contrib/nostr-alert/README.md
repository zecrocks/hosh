# Nostr Alert System

A Rust-based nostr client for monitoring the Hosh ZEC and BTC APIs and sending alerts via private messages when the servers lists become empty.

## Features

- Monitor Hosh ZEC and BTC APIs for server list availability
- Alert when either ZEC or BTC servers list becomes empty (critical monitoring failure)
- Send private message alerts to nostr admins
- Configurable check intervals
- Support for NIP04, NIP44, and NIP59 features
- Async/await support with Tokio
- Continuous monitoring with automatic reconnection

## Environment Variables

### Required
- `ADMIN_PUB_KEY` - The nostr public key of the admin to receive alerts (npub format)

### Optional
- `ZEC_API_URL` - The ZEC API URL to monitor (default: https://hosh.zec.rocks/api/v0/zec.json)
- `BTC_API_URL` - The BTC API URL to monitor (default: https://hosh.zec.rocks/api/v0/btc.json)
- `CHECK_INTERVAL_SECONDS` - How often to check the APIs in seconds (default: 60)
- `HOSH_PRIV_KEY` - Your nostr private key (nsec format). If not provided, a new keypair will be generated

## Dependencies

- `nostr-sdk = "0.42.0"` - Core nostr functionality
- `tokio = "1.0"` - Async runtime
- `reqwest = "0.11"` - HTTP client for API monitoring
- `serde_json = "1.0"` - JSON parsing
- `chrono = "0.4"` - Time handling and formatting
- `serde = "1.0"` - Serialization
- `anyhow = "1.0"` - Error handling

## Usage

### Building

```bash
cargo build
```

### Running Locally

```bash
# Set required environment variables
export ADMIN_PUB_KEY="npub1your_admin_public_key_here"

# Optional: Set API URLs to monitor (defaults to Hosh APIs)
export ZEC_API_URL="https://hosh.zec.rocks/api/v0/zec.json"
export BTC_API_URL="https://hosh.zec.rocks/api/v0/btc.json"

# Optional: Set check interval (in seconds)
export CHECK_INTERVAL_SECONDS="30"

# Optional: Set your private key (or let it generate a new one)
export HOSH_PRIV_KEY="nsec1your_private_key_here"

# Run the monitor
cargo run
```

### Running in Docker

```bash
docker compose -f docker-compose-dev.yml run -e ADMIN_PUB_KEY=npub1your_admin_key nostr-alert
```

### Running in Release Mode

```bash
cargo run --release
```

## How It Works

1. **Initialization**: Connects to multiple nostr relays and sets up the monitoring client
2. **Key Management**: Uses provided private key or generates a new one (saves the private key for reuse)
3. **Monitoring Loop**: 
   - Checks both ZEC and BTC APIs every `CHECK_INTERVAL_SECONDS`
   - Parses the JSON responses to count servers for each API
   - If either servers list is empty, sends a critical alert
   - If there's an error checking either API, sends an error alert
   - Continues monitoring indefinitely
4. **Alerts**: Sends detailed private messages including:
   - API URL being monitored
   - Critical status (empty servers list)
   - Error messages if API is unreachable
   - Timestamp of the issue

## Configuration

The system connects to multiple relays for reliability:
- wss://relay.damus.io
- wss://nostr.wine
- wss://relay.rip

## Development

This monitoring system demonstrates:
- Environment variable configuration
- JSON API monitoring and parsing
- Private message sending
- Continuous monitoring loops
- Error handling and logging
- Key management (generate or load existing)

## Future Enhancements

- Monitor individual server status changes
- Alert on performance degradation (high ping times)
- Track server availability trends
- Different alert types (public posts, encrypted messages)
- Alert throttling to prevent spam
- Integration with other monitoring systems
- Alert history and statistics
- Webhook integration 
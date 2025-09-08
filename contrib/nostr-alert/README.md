# Nostr Alert System

A Rust-based nostr client for monitoring the Hosh ZEC and BTC systems using both HTML pages and JSON APIs to detect monitoring failures and send alerts via private messages.

## Features

- **Dual Monitoring Approach**:
  - **HTML Pages**: Parse "Last Checked" column to detect stale checks
  - **JSON APIs**: Detect when server lists become completely empty
- Alert when either ZEC or BTC servers list becomes empty (critical monitoring failure)
- Alert when checks become stale (youngest check older than configurable threshold)
- Send private message alerts to nostr admins
- Configurable check intervals and stale check thresholds
- Support for NIP04, NIP44, and NIP59 features
- Async/await support with Tokio
- Continuous monitoring with automatic reconnection

## Environment Variables

### Required
- `ADMIN_PUB_KEY` - The nostr public key of the admin to receive alerts (npub format)

### Optional
- `ZEC_HTML_URL` - The ZEC HTML URL to monitor for stale checks (default: https://hosh.zec.rocks/zec)
- `BTC_HTML_URL` - The BTC HTML URL to monitor for stale checks (default: https://hosh.zec.rocks/btc)
- `ZEC_API_URL` - The ZEC JSON API URL to monitor for empty server lists (default: https://hosh.zec.rocks/api/v0/zec.json)
- `BTC_API_URL` - The BTC JSON API URL to monitor for empty server lists (default: https://hosh.zec.rocks/api/v0/btc.json)
- `CHECK_INTERVAL_SECONDS` - How often to check the endpoints in seconds (default: 60)
- `MAX_CHECK_AGE_MINUTES` - Maximum age of "Last Checked" times before alerting (default: 10, docker-compose default: 30)
- `HOSH_PRIV_KEY` - Your nostr private key (nsec format). If not provided, a new keypair will be generated

## Dependencies

- `nostr-sdk = "0.42.0"` - Core nostr functionality
- `tokio = "1.0"` - Async runtime
- `reqwest = "0.11"` - HTTP client for HTML monitoring
- `serde_json = "1.0"` - JSON parsing
- `chrono = "0.4"` - Time handling and formatting
- `serde = "1.0"` - Serialization
- `anyhow = "1.0"` - Error handling
- `regex = "1.0"` - HTML parsing

## Usage

### Building

```bash
cargo build
```

### Running Locally

```bash
# Set required environment variables
export ADMIN_PUB_KEY="npub1your_admin_public_key_here"

# Optional: Set HTML URLs to monitor (defaults to Hosh HTML pages)
export ZEC_HTML_URL="https://hosh.zec.rocks/zec"
export BTC_HTML_URL="https://hosh.zec.rocks/btc"

# Optional: Set check interval (in seconds)
export CHECK_INTERVAL_SECONDS="30"

# Optional: Set maximum check age before alerting (in minutes)
export MAX_CHECK_AGE_MINUTES="15"

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
3. **Dual Monitoring**: 
   - **HTML Pages**: Fetches ZEC and BTC HTML pages to parse "Last Checked" times
   - **JSON APIs**: Fetches ZEC and BTC JSON APIs to check for empty server lists
4. **Stale Check Detection**:
   - Converts "Last Checked" times to duration from HTML parsing
   - Finds the youngest (most recent) check among all servers
   - Compares against `MAX_CHECK_AGE_MINUTES` threshold
   - Alerts if the youngest check is older than threshold
5. **Empty List Detection**:
   - Parses JSON API responses to count total servers
   - Alerts if server list is completely empty (critical failure)
6. **Alerts**: Sends detailed private messages including:
   - HTML and API URLs being monitored
   - Critical status (empty servers list)
   - Stale check warnings with youngest check time
   - Error messages if endpoints are unreachable
   - Timestamp of the issue

## Alert Types

### 🚨 Critical Alert (Empty Servers List)
```
🚨 CRITICAL ALERT: ZEC SERVERS LIST IS EMPTY!

API URL: https://hosh.zec.rocks/api/v0/zec.json
Time: 2024-01-15 14:30:25 UTC

This indicates a critical failure in the ZEC monitoring system.
```

### 🚨 Stale Checks Warning
```
🚨 WARNING: ZEC CHECKS ARE STALE!

HTML URL: https://hosh.zec.rocks/zec
Time: 2024-01-15 14:30:25 UTC
Youngest check: 25 minutes
Max allowed age: 10 minutes

This indicates the monitoring system may have stopped working.
```

### ✅ Recovery Notification
```
✅ ZEC RECOVERED

API URL: https://hosh.zec.rocks/api/v0/zec.json
HTML URL: https://hosh.zec.rocks/zec
Time: 2024-01-15 14:30:25 UTC
Status: 21 servers found

The ZEC monitoring system is back online.
```

### ❌ Error Alert
```
🚨 ZEC MONITORING ERROR

HTML URL: https://hosh.zec.rocks/zec
API URL: https://hosh.zec.rocks/api/v0/zec.json
Time: 2024-01-15 14:30:25 UTC

Both HTML and JSON endpoints are unreachable.
```

## Configuration

The system connects to multiple relays for reliability:
- wss://relay.damus.io
- wss://nostr.wine
- wss://relay.rip

## HTML Parsing

The system parses HTML table rows to extract:
- Server hostname (from the first column link)
- Last checked time (from the 5th column)

Supported time formats:
- "Just now" or "0s" → 0 seconds
- "4m 21s" → 4 minutes 21 seconds
- "1h 30m" → 1 hour 30 minutes
- "2d 5h" → 2 days 5 hours

## JSON API Monitoring

The system also monitors JSON API endpoints to detect:
- Empty server lists (critical failure)
- Total server counts for recovery notifications
- API availability and response validity

## Monitoring Logic

The system uses a sophisticated approach to determine health:

1. **Both HTML and JSON available**: 
   - Check JSON first for empty server lists (critical)
   - Check HTML for stale checks (warning)
2. **Only HTML available**: Check for stale checks
3. **Only JSON available**: Check for empty server lists
4. **Neither available**: Report error state

This ensures comprehensive monitoring even if one endpoint fails. 
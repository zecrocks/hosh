# Nostr Alert Service

The Nostr Alert Service is a monitoring component that continuously checks the health of ZEC and BTC APIs and sends real-time alerts via Nostr direct messages when critical issues are detected.

## Overview

The service monitors two critical endpoints:
- **ZEC API**: `http://web:8080/api/v0/zec.json` (local container)
- **BTC API**: `http://web:8080/api/v0/btc.json` (local container)

When the server list becomes empty (indicating a critical monitoring failure), the service immediately sends a direct message to a configured admin via Nostr.

## Features

### Intelligent Alerting
- **State Change Detection**: Only sends alerts when status changes, preventing alert fatigue
- **Recovery Notifications**: Sends notifications when services recover from downtime
- **Error Handling**: Alerts on API connectivity issues and parsing errors

### Status Transitions
- **Healthy → Empty**: Critical alert sent
- **Empty → Healthy**: Recovery notification sent
- **Healthy → Error**: Error alert sent
- **Error → Healthy**: Recovery notification sent

### Privacy & Security
- **Discreet Operation**: No metadata broadcast when using existing keys
- **Secure Communication**: Uses Nostr's encrypted direct messages
- **Key Management**: Supports both existing and generated keypairs

## Configuration

### Environment Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `ADMIN_PUB_KEY` | Nostr public key of admin to receive alerts | - | Yes |
| `HOSH_PRIV_KEY` | Private key for sending alerts (nsec format) | - | Yes* |
| `GENERATE_KEYS` | Generate new keypair if true | `false` | No |
| `ZEC_API_URL` | ZEC API endpoint to monitor | `http://web:8080/api/v0/zec.json` | No |
| `BTC_API_URL` | BTC API endpoint to monitor | `http://web:8080/api/v0/btc.json` | No |
| `CHECK_INTERVAL_SECONDS` | Monitoring interval in seconds | `300` | No |

*Required unless `GENERATE_KEYS=true`

### Key Management

#### Using Existing Keys
```bash
export ADMIN_PUB_KEY="npub1your_admin_public_key_here"
export HOSH_PRIV_KEY="nsec1your_private_key_here"
export GENERATE_KEYS=false
```

#### Generating New Keys
```bash
export ADMIN_PUB_KEY="npub1your_admin_public_key_here"
export GENERATE_KEYS=true
# HOSH_PRIV_KEY not needed - will be generated
```

## Usage

### Development Mode
```bash
# Start with hot-reloading
docker compose --profile dev up nostr-alert
```

### Production Mode
```bash
# Start production service
docker compose up nostr-alert
```

### Local Development
```bash
cd contrib/nostr-alert
cargo run
```

### Monitoring Different Environments

#### Local Development (Default)
The service is configured by default to monitor the local web container:
```bash
# Default configuration (no environment variables needed)
ZEC_API_URL=http://web:8080/api/v0/zec.json
BTC_API_URL=http://web:8080/api/v0/btc.json
```

#### Production Monitoring
To monitor the production APIs, override the default URLs:
```bash
export ZEC_API_URL="https://hosh.zec.rocks/api/v0/zec.json"
export BTC_API_URL="https://hosh.zec.rocks/api/v0/btc.json"
```

#### Custom Endpoints
Monitor any API endpoints by setting the environment variables:
```bash
export ZEC_API_URL="https://your-custom-api.com/zec.json"
export BTC_API_URL="https://your-custom-api.com/btc.json"
```

## Alert Messages

### Critical Alert (Empty Server List)
```
🚨 CRITICAL ALERT: ZEC SERVERS LIST IS EMPTY!

API URL: http://web:8080/api/v0/zec.json
Time: 2024-01-15 14:30:25 UTC

This indicates a critical failure in the ZEC monitoring system.
```

### Recovery Notification
```
✅ ZEC API RECOVERED

API URL: http://web:8080/api/v0/zec.json
Time: 2024-01-15 14:35:10 UTC
Status: 16/21 servers online

The ZEC monitoring system is back online.
```

### Error Alert
```
🚨 ZEC API MONITORING ERROR

API URL: http://web:8080/api/v0/zec.json
Error: HTTP error: 404 Not Found
Time: 2024-01-15 14:30:25 UTC
```

## Architecture

### Components
- **Rust Application**: Core monitoring logic using `nostr-sdk`
- **Nostr Client**: Connects to multiple relays for message delivery
- **HTTP Client**: Monitors API endpoints with timeout handling
- **State Tracker**: Maintains previous states to detect changes

### Relays
The service connects to multiple Nostr relays for reliable message delivery:
- `wss://relay.damus.io`
- `wss://nostr.wine`
- `wss://relay.rip`

### Signal Handling
- Graceful shutdown on `Ctrl+C`
- Proper relay disconnection
- Clean process termination

## Integration

### Docker Compose
The service is integrated into the Hosh Docker Compose setup:

```yaml
nostr-alert:
  build:
    context: ./contrib/nostr-alert
    dockerfile: prod.Dockerfile
  environment:
    - ADMIN_PUB_KEY=${ADMIN_PUB_KEY}
    - HOSH_PRIV_KEY=${HOSH_PRIV_KEY}
    - ZEC_API_URL=${ZEC_API_URL}
    - BTC_API_URL=${BTC_API_URL}
    - CHECK_INTERVAL_SECONDS=${CHECK_INTERVAL_SECONDS}
    - GENERATE_KEYS=${GENERATE_KEYS}
  restart: unless-stopped
```

### Development Workflow
1. **Setup**: Configure environment variables
2. **Test**: Run in development mode with hot-reloading
3. **Deploy**: Use production Docker image
4. **Monitor**: Check logs for alert delivery status

## Troubleshooting

### Common Issues

**Invalid Private Key Format**
```
❌ Error: Invalid HOSH_PRIV_KEY format: Invalid secret key
```
Solution: Ensure key is in `nsec` format or set `GENERATE_KEYS=true`

**Missing Admin Public Key**
```
❌ Error: ADMIN_PUB_KEY environment variable is required
```
Solution: Set the admin's Nostr public key in `npub` format

**API Connection Issues**
```
🚨 ERROR CHECKING ZEC API: HTTP error: 404 Not Found
```
Solution: Verify API URLs are correct and accessible

### Log Levels
- **Info**: Normal operation and status updates
- **Error**: Connection issues and alert failures
- **Debug**: Detailed relay connection status

## Future Enhancements

- **Multiple Admin Support**: Alert multiple administrators
- **Custom Alert Thresholds**: Configure different alert conditions
- **Alert History**: Persistent storage of alert history
- **Webhook Integration**: Additional notification channels
- **Metrics Collection**: Monitoring service performance metrics 
# HTTP Block Explorer Checker

This service monitors various blockchain block explorers to track block heights across different networks. It scrapes data from multiple sources and stores it in Redis for further processing.

## Overview

The checker fetches block height data from several block explorer websites:
- Blockchair.com
- Blockchair.onion
- Blockchain.com
- Blockstream.info
- Zec.rocks
- ZcashExplorer.app

## Redis Key Format

Keys are stored in Redis using the format: `http:{source}.{chain}`

where:
- `source` is the explorer name (e.g., blockchair, blockchain, blockstream)
- `chain` is the canonical chain identifier from the explorer's URL

Examples:
- `http:blockchair.bitcoin` (from https://blockchair.com/bitcoin)
- `http:blockchair.ethereum` (from https://blockchair.com/ethereum)
- `http:blockchair.bitcoin-cash` (from https://blockchair.com/bitcoin-cash)
- `http:blockchain.bitcoin`
- `http:blockstream.bitcoin`
- `http:blockstream.liquid-network`
- `http:zecrocks.zcash`
- `http:zcashexplorer.zcash`

## How It Works

1. The service subscribes to NATS messages on the topic `{prefix}check.http`
2. When triggered, it concurrently fetches data from all sources
3. Results are stored in Redis using the format: `http:{source}.{chain}`

### Data Sources

#### Blockchair
- Supports multiple chains including Bitcoin, Ethereum, Zcash, and many others
- Fetches from both clearnet (blockchair.com) and Tor (.onion) sites
- Example key: `http:blockchair.ethereum`

#### Blockchain.com
- Supports Bitcoin, Ethereum, and Bitcoin Cash
- Example key: `http:blockchain.bitcoin`

#### Blockstream
- Supports Bitcoin and Liquid Network
- Example key: `http:blockstream.bitcoin`

#### Zcash-specific Explorers
- Zec.rocks: `http:zecrocks.zcash`
- ZcashExplorer: `http:zcashexplorer.zcash`

## Configuration

The service can be configured using environment variables:
- `NATS_HOST`: NATS server hostname (default: "nats")
- `NATS_PORT`: NATS server port (default: 4222)
- `REDIS_HOST`: Redis server hostname (default: "redis")
- `REDIS_PORT`: Redis server port (default: 6379)
- `NATS_PREFIX`: Prefix for NATS topics (default: "hosh.")
- `TOR_PROXY_HOST`: Tor proxy hostname (default: "tor")
- `TOR_PROXY_PORT`: Tor proxy port (default: 9050)

## Error Handling

The service implements robust error handling:
- Connection errors (Redis/NATS) are logged and retried
- Parser errors for each explorer are handled independently
- Failed scrapes for one source don't affect other sources

## Development

The checker is written in Rust and uses:
- `reqwest` for HTTP requests (with SOCKS5 proxy support for Tor)
- `scraper` for HTML parsing
- `redis` for data storage
- `async-nats` for message queue integration
- `tokio` for async runtime

Each explorer implementation is in its own module:
- `blockchair/`
  - `blockchairdotcom.rs` - Clearnet implementation
  - `blockchairdotonion.rs` - Tor implementation
  - `mod.rs` - Module exports
- `blockchain.rs`
- `blockstream.rs`
- `zecrocks.rs`
- `zcashexplorer.rs`

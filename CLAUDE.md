# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Hosh is a monitoring system for light wallet server uptime across Bitcoin (Electrum) and Zcash (lightwalletd) blockchains. Live demo: https://hosh.zec.rocks

## Development Commands

```bash
# Start all services (production mode)
docker compose up

# Start with all-in-one container (simpler, 3 containers instead of 7)
docker compose -f docker-compose-all.yml up

# Development mode with hot-reload (all-in-one)
docker compose -f docker-compose-dev-all.yml up

# Development mode with separate services
docker compose -f docker-compose-dev.yml up

# Apply database migrations
docker compose exec chronicler /docker-entrypoint-initdb.d/init-db.sh

# Build workspace locally
cargo build --workspace

# Build release binary
cargo build --release -p hosh

# Run unified binary with specific roles (comma-separated)
./target/release/hosh --roles web
./target/release/hosh --roles checker-btc
./target/release/hosh --roles checker-zec
./target/release/hosh --roles discovery
./target/release/hosh --roles all  # or just ./target/release/hosh
./target/release/hosh --roles web,discovery  # run multiple roles
./target/release/hosh --roles web,checker-zec,checker-btc  # any combination
```

## Architecture

The system is built as a **unified Rust binary** with role-based execution, orchestrated via Docker Compose:

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Discovery  │────▶│  Chronicler │◀────│     Web     │
└─────────────┘     │ (ClickHouse)│     └─────────────┘
                    └─────────────┘            ▲
                          ▲                    │
        ┌─────────────────┼─────────────────┐  │
        │                 │                 │  │
┌───────┴─────┐   ┌───────┴─────┐   ┌───────┴──┘
│ checker-btc │   │ checker-zec │   │    Tor    │
└─────────────┘   └─────────────┘   └───────────┘
```

### Workspace Structure

```
hosh/
├── Cargo.toml              # Workspace root
├── Dockerfile              # Unified multi-stage build
├── docker-compose.yml      # Production: separate containers per role
├── docker-compose-all.yml  # Production: all roles in one container
├── docker-compose-dev.yml  # Development: separate services with hot-reload
├── docker-compose-dev-all.yml  # Development: all roles with hot-reload
├── crates/
│   ├── hosh/               # Main binary with CLI (clap)
│   ├── hosh-core/          # Shared library (config, types, clickhouse)
│   ├── hosh-web/           # Web service (Actix-web, Askama templates)
│   ├── hosh-checker-btc/   # BTC Electrum checker
│   ├── hosh-checker-zec/   # ZEC lightwalletd checker (gRPC)
│   └── hosh-discovery/     # Server discovery service
├── chronicler/             # ClickHouse database setup
└── contrib/                # Additional tools (dashboard, nostr-alert)
```

### Core Services

- **hosh** (unified binary): All services in one binary, roles selected via `--roles` flag
  - `--roles web`: HTTP server with HTML dashboard and JSON API. Port 8080.
  - `--roles checker-btc`: Polls jobs and checks Electrum servers
  - `--roles checker-zec`: Checks Zcash lightwalletd servers via gRPC
  - `--roles discovery`: Discovers and registers new servers
  - `--roles all` (default): Runs all roles concurrently
  - Supports comma-separated values: `--roles web,discovery,checker-zec`
- **chronicler** (ClickHouse): Time-series database storing check results and targets
- **tor**: SOCKS proxy for accessing .onion hidden services

### Data Flow

1. **Discovery** registers targets in ClickHouse `targets` table
2. **Web** API serves jobs to checkers via `/api/v1/jobs`
3. **Checkers** perform health checks and store results in ClickHouse `results` table
4. **Web** reads results for dashboard display and API responses

### Key Files

- `crates/hosh/src/main.rs`: CLI entry point with role selection
- `crates/hosh-core/src/`: Shared types, config, ClickHouse client
- `crates/hosh-web/templates/`: Askama HTML templates
- `crates/hosh-discovery/src/lib.rs`: Static ZEC server list with community servers
- `chronicler/migrations/`: Database schema (results, targets, uptime_stats tables)

## Environment Configuration

Copy `.env.example` to `.env`. Required variables:
- `CLICKHOUSE_DB`, `CLICKHOUSE_USER`, `CLICKHOUSE_PASSWORD`: Database credentials
- `API_KEY`: Authentication for checker services

## Tech Stack

- **Language**: Rust 1.87.0 (see `rust-toolchain.toml`)
- **Web Framework**: Actix-web with Askama templates
- **Database**: ClickHouse (time-series data)
- **Protocols**: gRPC (Zcash), Electrum protocol (Bitcoin)
- **Container**: Docker Compose with unified image

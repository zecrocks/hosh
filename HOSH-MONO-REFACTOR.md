# Hosh Monorepo Consolidation

This document summarizes the refactor that consolidated all Rust services into a single unified binary.

## Overview

All production Rust services (web, checker-btc, checker-zec, discovery) were consolidated into ONE binary with role-based execution via CLI flags.

**Before:** 4 separate Rust projects, 4 Dockerfiles, 4 Docker images
**After:** 1 Cargo workspace, 1 Dockerfile, 1 Docker image

## Usage

```bash
./hosh --role web           # Run web server only
./hosh --role checker-btc   # Run BTC checker only
./hosh --role checker-zec   # Run ZEC checker only
./hosh --role discovery     # Run discovery only
./hosh                      # Run ALL roles concurrently (default)
```

## New Directory Structure

```
hosh/
├── Cargo.toml              # Workspace root with shared dependencies
├── Dockerfile              # Unified multi-stage Docker build
├── dev.Dockerfile          # Development Dockerfile with cargo-watch
├── docker-compose.yml      # Updated to use unified image
├── docker-compose-dev.yml  # Updated for dev mode
└── crates/
    ├── hosh/               # Main binary with CLI entry point
    │   ├── Cargo.toml
    │   └── src/main.rs     # clap-based CLI, role selection
    ├── hosh-core/          # Shared library
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── config.rs       # Unified environment config
    │       ├── clickhouse.rs   # Shared ClickHouse client
    │       ├── types.rs        # CheckRequest, CheckResult, etc.
    │       └── tracing_setup.rs
    ├── hosh-web/           # Web service module
    │   ├── Cargo.toml
    │   ├── src/lib.rs      # Exposes run() function
    │   ├── templates/      # Askama HTML templates
    │   └── static/         # Static assets
    ├── hosh-checker-btc/   # BTC checker module
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs      # Exposes run() function
    │       ├── worker.rs   # Job polling and checking logic
    │       ├── utils.rs
    │       └── routes/     # API routes (optional server mode)
    ├── hosh-checker-zec/   # ZEC checker module
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs      # Exposes run() function
    │       └── socks_connector.rs
    └── hosh-discovery/     # Discovery module
        ├── Cargo.toml
        └── src/lib.rs      # Exposes run() function

```

## What Was Removed

- `web/` - Migrated to `crates/hosh-web/`
- `checkers/btc/` - Migrated to `crates/hosh-checker-btc/`
- `checkers/zec/` - Migrated to `crates/hosh-checker-zec/`
- `discovery/` - Migrated to `crates/hosh-discovery/`
- Individual Dockerfiles from each service directory

## Key Implementation Details

### Main Binary (`crates/hosh/src/main.rs`)

```rust
use clap::Parser;

#[derive(Parser)]
#[command(name = "hosh", about = "Light wallet server monitoring")]
struct Cli {
    #[arg(long, short, default_value = "all")]
    role: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    hosh_core::tracing_setup::init();
    let cli = Cli::parse();
    
    match cli.role.as_str() {
        "web" => hosh_web::run().await?,
        "checker-btc" => hosh_checker_btc::run().await?,
        "checker-zec" => hosh_checker_zec::run().await?,
        "discovery" => hosh_discovery::run().await?,
        "all" => {
            tokio::try_join!(
                hosh_web::run(),
                hosh_checker_btc::run(),
                hosh_checker_zec::run(),
                hosh_discovery::run(),
            )?;
        }
        _ => {
            eprintln!("Unknown role: {}", cli.role);
            std::process::exit(1);
        }
    }
    Ok(())
}
```

### Unified Dockerfile

Multi-stage build using cargo-chef for optimal caching:

```dockerfile
FROM rust:1.87-slim-bullseye AS chef
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# Install all build deps (protobuf for gRPC, etc.)
RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev git make protobuf-compiler
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release -p hosh

FROM debian:bullseye-slim
COPY --from=builder /app/target/release/hosh /app/hosh
COPY --from=builder /app/crates/hosh-web/templates /app/templates
COPY --from=builder /app/crates/hosh-web/static /app/static
ENTRYPOINT ["/app/hosh"]
CMD ["--role", "all"]
```

### Docker Compose Changes

Services now use the same image with different `--role` commands:

```yaml
services:
  web:
    build: .
    image: hosh/hosh
    command: ["--role", "web"]
    
  checker-btc:
    build: .
    image: hosh/hosh
    command: ["--role", "checker-btc"]
    
  checker-zec:
    build: .
    image: hosh/hosh
    command: ["--role", "checker-zec"]
    
  discovery:
    build: .
    image: hosh/hosh
    command: ["--role", "discovery"]
```

## Shared Dependencies

The workspace `Cargo.toml` defines shared dependencies used across crates:

| Category | Crates |
|----------|--------|
| Async Runtime | tokio |
| Web Framework | actix-web, askama (hosh-web) |
| gRPC | tonic, zcash_client_backend (hosh-checker-zec) |
| Electrum | bdk, electrum-client, bitcoin (hosh-checker-btc) |
| SOCKS/Tor | tokio-socks |
| HTML Scraping | scraper (hosh-discovery) |
| Common | serde, chrono, tracing, reqwest, uuid |

## Development Commands

```bash
# Build workspace
cargo build --workspace

# Build release binary
cargo build --release -p hosh

# Check all crates
cargo check --workspace

# Run locally with specific role
cargo run -p hosh -- --role web

# Docker development mode
ln -s docker-compose-dev.yml docker-compose.override.yml
docker compose up
```

## Benefits

1. **Single Docker image** - Simpler CI/CD, smaller total image size
2. **Shared code** - Common types and utilities in `hosh-core`
3. **Consistent dependencies** - Workspace-level dependency management
4. **Flexible deployment** - Run any combination of services
5. **Easier development** - One `cargo build` compiles everything

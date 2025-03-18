# Hosh Web Interface

The web interface for Hosh provides real-time status monitoring of light wallet servers across different blockchain networks (Bitcoin, Zcash). It's built with Rust using Actix-web and integrates with Redis for data storage.

## Data Presentation

### Server Status Dashboard

The main dashboard displays a comprehensive table of server information:

- **Server**: Hostname and port, clickable for detailed view
- **Block Height**: Current blockchain height with status indicators
  - ⚠️ Warning icon for servers behind the 90th percentile
  - ℹ️ Info icon for servers ahead of the 90th percentile
- **Status**: Online/Offline status with color coding
- **Version**: Server software version information
- **Last Checked**: Time since last successful check
- **USA Ping**: Response time in milliseconds

The dashboard header shows:
- Total servers online (e.g., "14/14 servers online")
- 90th percentile block height for network consensus

### Block Explorer Heights

A separate view compares block heights across different blockchain explorers:

- **Blockchair**: Multi-chain explorer with both clearnet and .onion services
- **Blockchain.com**: Bitcoin, Ethereum, and Bitcoin Cash data
- **Blockstream**: Bitcoin and Liquid Network
- **Specialized Explorers**: Zec.rocks and Zcash Explorer for Zcash

Features:
- Height differences shown in parentheses (e.g., "+146" blocks)
- Visual indicators for each blockchain (e.g., ₿ for Bitcoin, ⓩ for Zcash)
- Explorer logos and links to their respective websites

## Architecture

### Core Components

1. **Web Server (Actix-web)**
   - Handles HTTP requests and routing
   - Serves static files from `/static` directory
   - Provides both HTML and JSON API endpoints
   - Runs on port 8080 by default

2. **Template Engine (Askama)**
   - Type-safe templating system
   - Templates are compiled at build time
   - Located in `templates/` directory:
     - `layout.html` - Base layout with navigation
     - `index.html` - Main dashboard view
     - `server.html` - Individual server details
     - `check.html` - Server check results
     - `blockchain_heights.html` - Explorer heights comparison

3. **Data Models**
   - `ServerInfo`: Core data structure for server status
   - `SafeNetwork`: Network type validator (BTC/ZEC/HTTP)
   - Various template structs (`IndexTemplate`, `ServerTemplate`, etc.)

### Redis Integration

The application uses Redis as its primary data store:

1. **Connection Setup**
   ```rust
   let redis_url = format!("redis://{}:{}", redis_host, redis_port);
   let redis_client = redis::Client::open(redis_url.as_str());
   ```

2. **Data Storage Format**
   - Network-specific keys: `{network}:*` (e.g., `btc:*`, `zec:*`)
   - Explorer data: `http:{source}.{chain}`
   - Server data stored as JSON strings

3. **Key Operations**
   - Fetching server lists: `conn.keys(network.redis_prefix())`
   - Reading server data: `conn.get(&key)`
   - Data is deserialized into `ServerInfo` structs

### Template System (Askama)

1. **Template Structure**
   - Templates are defined using the `#[derive(Template)]` attribute
   - Each template has an associated struct defining its context
   ```rust
   #[derive(Template)]
   #[template(path = "index.html")]
   struct IndexTemplate<'a> {
       servers: Vec<ServerInfo>,
       percentile_height: u64,
       // ...
   }
   ```

2. **Custom Filters**
   - Located in `filters` module
   - Provides utilities like:
     - `filter`: Custom filtering for collections
     - `format_value`: JSON value formatting
     - `first`: Array first element accessor

3. **Template Inheritance**
   - Base templates: `layout.html` and `base.html`
   - Child templates extend base using `{% extends "layout.html" %}`
   - Blocks system for content organization

## Environment Variables

- `REDIS_HOST`: Redis server hostname (default: "redis")
- `REDIS_PORT`: Redis server port (default: 6379)

## API Endpoints

1. **HTML Pages**
   - `/`: Redirects to `/zec`
   - `/{network}`: Dashboard for specific network
   - `/{network}/{server}`: Individual server details
   - `/explorers`: Block explorer heights comparison

2. **JSON API**
   - `/api/v0/{network}.json`: Server status in JSON format

## Development

1. **Building**
   ```bash
   cargo build
   ```

2. **Running**
   ```bash
   cargo run
   ```

3. **Docker**
   ```bash
   docker build -t hosh/web .
   docker run -p 8080:8080 hosh/web
   ```

## Dependencies

- `actix-web`: Web framework
- `askama`: Template engine
- `redis`: Redis client
- `serde`: Serialization/deserialization
- `chrono`: DateTime handling
- `tracing`: Logging and diagnostics 
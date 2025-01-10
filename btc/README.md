# BTC Worker

The **BTC Worker** is a Python-based microservice designed to interact with Electrum servers. It leverages Flask to provide a REST API for querying server details and provides enhanced integration for .onion services via the Tor network.

## Purpose

The BTC Worker facilitates:
- Querying Electrum servers to obtain blockchain information.
- Resolving IP addresses or .onion addresses of Electrum servers.
- Fetching blockchain headers, server features, and more using JSON-RPC over plain or SSL connections.
- Utilizing Tor to handle .onion addresses securely.

This service is particularly useful for managing distributed Electrum servers, testing server availability, and retrieving blockchain-related metadata.

## Features

1. **Tor Network Integration**:
   - Supports connections to .onion addresses using a Tor SOCKS proxy.
   - Automatically handles Tor proxy configuration for .onion addresses.

2. **RESTful API**:
   - `GET /electrum/query`: Queries Electrum servers with specified methods.
   - `GET /electrum/servers`: Fetches a list of available Electrum servers.

3. **Redis Integration**:
   - Can be combined with a Redis instance for caching server responses.

4. **Dynamic Sorting**:
   - Sorts Electrum servers dynamically by priority (URL, .onion, IP).

5. **Error Handling**:
   - Provides detailed error responses when a server is unreachable or methods fail.

## Technical Details

### Architecture
- **Language**: Python (3.11)
- **Frameworks**:
  - Flask: REST API framework.
  - Flasgger: Auto-generates Swagger documentation for the API.
- **Network Support**:
  - Tor: Used for anonymized .onion connections.
  - SSL: Secure Electrum connections.

### File Descriptions

#### `api.py`
- The main application file defining API routes.
- Handles querying Electrum servers and resolving their details.

#### `Dockerfile`
- Configures a containerized environment for the BTC Worker.
- Installs necessary dependencies, including Python libraries and Electrum.

#### `entrypoint.sh`
- Sets up the Electrum daemon on container startup.
- Ensures any leftover lock files are cleaned before initialization.
- Starts the Flask application.

### Key Libraries
- `socks`: Used for Tor proxy integration.
- `ssl`: Enables secure connections to Electrum servers.
- `subprocess`: Executes Electrum commands directly when necessary.

### Deployment

1. Build the Docker image:

```bash
docker build -t btc-worker .
```

2. Run the container

```bash
docker run -d --name btc-worker -p 5000:5000 --network <network> btc-worker
```


### Example Usage

#### Query an Electrum Server
```bash
curl -X GET "http://localhost:5000/electrum/query?url=<server-url>&port=<port>"
```

#### Fetch Server List
```bash
curl -X GET "http://localhost:5000/electrum/servers"
```

## Environment Variables
- `REDIS_HOST`: Hostname for the Redis instance (default: `redis`).
- `REDIS_PORT`: Port for Redis communication (default: `6379`).

## Future Enhancements
- Implement caching for server responses to reduce repeated queries.
- Add additional JSON-RPC methods support.


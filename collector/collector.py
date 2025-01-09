import redis
import json
import time
import requests
import os
from utils import parse_block_header
import datetime

# Environment Variables
BTC_WORKER = os.environ.get('BTC_WORKER', 'http://btc-worker:5000')  # assume docker compose stack if not specified
SERVER_REFRESH_INTERVAL_SECONDS = int(os.environ.get('SERVER_REFRESH_INTERVAL_SECONDS', 5 * 60))  # how often to refresh servers
UPDATE_INTERVAL_SECONDS = int(os.environ.get('UPDATE_INTERVAL', 10))  # how often to check for server refresh

# Redis Configuration
REDIS_HOST = os.environ.get('REDIS_HOST', 'redis')
REDIS_PORT = int(os.environ.get('REDIS_PORT', 6379))

# Connect to Redis
try:
    redis_client = redis.StrictRedis(host=REDIS_HOST, port=REDIS_PORT, db=0, socket_timeout=5)
    # Verify connection
    redis_client.ping()
    print("Connected to Redis successfully!")
except redis.exceptions.ConnectionError as e:
    print(f"Failed to connect to Redis: {e}")
    exit(1)


def make_json_serializable(data):
    for key, value in data.items():
        if isinstance(value, datetime.datetime):
            data[key] = value.isoformat()
    return data


# Fetch server list from Electrum
def fetch_servers():
    try:
        response = requests.get(f"{BTC_WORKER}/electrum/servers", timeout=10)
        servers = response.json().get("servers", {})
        return servers
    except Exception as e:
        print(f"Error fetching servers: {e}")
        return {}


# Query data from a single Electrum server
def query_server_data(host, port=50002, version="unknown"):
    url = f"{BTC_WORKER}/electrum/query"
    params = {
        "url": host,
        "method": "blockchain.headers.subscribe",
        "port": port
    }
    try:
        # Query the API
        response = requests.get(url, params=params, timeout=10)

        # Validate response
        if response.status_code != 200:
            raise Exception(f"HTTP {response.status_code}")

        # Parse JSON response
        data = response.json()

        # Extract ping, self_signed status, and result
        ping = data.get("ping", "N/A")
        self_signed = data.get("self_signed", False)  # Add self_signed field
        result = data.get("response", {}).get("result", {})

        # Check if required fields exist
        if "hex" not in result.get("result", {}):
            return {
                "Ping": ping,
                "error": "Malformed response: missing 'hex' field",
                "host": host,
                "port": port,
                "version": version,
                "LastUpdated": datetime.datetime.utcnow().isoformat(),
                "response_details": data  # Include full response details
            }


        # Parse block header
        parsed_header = parse_block_header(result.get("hex", ""))

        # Combine data
        return {
            "Ping": ping,
            "self_signed": self_signed,  # Include self_signed status
            "Height": result.get("height", "N/A"),
            "Timestamp": parsed_header.get("timestamp_human", "N/A"),
            "Difficulty Bits": parsed_header.get("bits", "N/A"),
            "Nonce": parsed_header.get("nonce", "N/A"),
            "host": host,
            "port": port,
            "version": version,  # Include version
            "LastUpdated": datetime.datetime.utcnow().isoformat()
        }
    except requests.exceptions.Timeout:
        return {
            "Ping": "Timeout",
            "self_signed": False,  # Default to False in case of timeout
            "error": "Timeout while querying server",
            "host": host,
            "port": port,
            "version": version,  # Include version
            "LastUpdated": datetime.datetime.utcnow().isoformat()
        }
    except Exception as e:
        return {
            "Ping": "N/A",
            "self_signed": False,  # Default to False in case of failure
            "error": f"All methods failed or server is unreachable: {str(e)}",
            "host": host,
            "port": port,
            "version": version,  # Include version
            "LastUpdated": datetime.datetime.utcnow().isoformat()
        }


# Check if a key is stale
def is_stale(key):
    raw_data = redis_client.get(key)
    if not raw_data:
        return True

    try:
        data = json.loads(raw_data)
        last_updated = data.get("LastUpdated", None)
        if not last_updated:
            return True

        last_updated_time = datetime.datetime.fromisoformat(last_updated)
        age = (datetime.datetime.utcnow() - last_updated_time).total_seconds()
        return age > SERVER_REFRESH_INTERVAL_SECONDS
    except Exception as e:
        print(f"Error parsing timestamp for key {key}: {e}")
        return True


# Main loop to collect data and store in Redis
def main_loop():
    while True:
        servers = fetch_servers()
        if not servers:
            print(f"No servers found. Sleeping for {UPDATE_INTERVAL_SECONDS} seconds...")
            time.sleep(UPDATE_INTERVAL_SECONDS)
            continue

        for host, details in servers.items():
            port = details.get("s", 50002)  # Default to SSL port if not specified
            version = details.get("version", "unknown")

            # Check if the key is stale or doesn't exist
            if redis_client.exists(host) and not is_stale(host):
                print(f"Skipping server {host}: data is fresh in Redis")
                continue

            print(f"Processing server: {host}")
            server_data = query_server_data(host, port, version)
            server_data = make_json_serializable(server_data)

            try:
                # Publish to Redis
                redis_client.set(host, json.dumps(server_data))
                print(f"Data for server {host} saved to Redis.")
            except redis.exceptions.ConnectionError as e:
                print(f"Redis connection error while saving data for {host}: {e}")
            except Exception as e:
                print(f"Unexpected error saving data for {host}: {e}")

        print(f"Finished one cycle. Sleeping for {UPDATE_INTERVAL_SECONDS} seconds...")
        time.sleep(UPDATE_INTERVAL_SECONDS)


if __name__ == "__main__":
    main_loop()



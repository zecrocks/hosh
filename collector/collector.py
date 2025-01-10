import redis
import json
import time
import requests
import os
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
def query_server_data(host, port=50002, electrum_version="unknown"):
    url = f"{BTC_WORKER}/electrum/query"
    params = {
        "url": host,
        "method": "blockchain.headers.subscribe",
        "port": port
    }

    # Query the API
    response = requests.get(url, params=params, timeout=10)

    # Validate response
    if response.status_code != 200:
        raise Exception(f"HTTP {response.status_code}")

    # Parse JSON response
    data = response.json()

    # Add metadata fields and return the result
    data.update({
        "host": host,
        "port": port,
        "electrum_version": electrum_version,
        "LastUpdated": datetime.datetime.utcnow().isoformat()
    })
    return data



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
            if '.onion' in host:
                # skip tor for now
                continue
            port = details.get("s", 50002)  # Default to SSL port if not specified
            electrum_version = details.get("version", "unknown")

            # Check if the key is stale or doesn't exist
            if redis_client.exists(host) and not is_stale(host):
                print(f"Skipping server {host}: data is fresh in Redis")
                continue

            print(f"Processing server: {host}")
            try:
                server_data = query_server_data(host, port, electrum_version)
            except:
                print(f"could not fetch from {BTC_WORKER}")
                continue
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


import redis
import json
import time
import requests
import os
import datetime
import asyncio
import nats
from nats.errors import ConnectionClosedError, TimeoutError, NoServersError

# Environment Variables
BTC_WORKER = os.environ.get('BTC_WORKER', 'http://btc-worker:5000')
SERVER_REFRESH_INTERVAL_SECONDS = int(os.environ.get('CHECK_INTERVAL', 300))  # 5 minutes default
NATS_URL = os.environ.get('NATS_URL', 'nats://nats:4222')
NATS_SUBJECT = os.environ.get('NATS_SUBJECT', 'hosh.check')

# Redis Configuration
REDIS_HOST = os.environ.get('REDIS_HOST', 'redis')
REDIS_PORT = int(os.environ.get('REDIS_PORT', 6379))

# Connect to Redis
try:
    redis_client = redis.StrictRedis(host=REDIS_HOST, port=REDIS_PORT, db=0, socket_timeout=5)
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


async def process_check_request(msg):
    """Handle incoming check requests from NATS"""
    try:
        data = json.loads(msg.data.decode())
        host = data['host']
        port = data.get('port', 50002)
        electrum_version = data.get('version', 'unknown')

        print(f"Processing check request for server: {host}")
        
        # Skip if data is fresh
        if redis_client.exists(host) and not is_stale(host):
            print(f"Skipping server {host}: data is fresh in Redis")
            return

        try:
            server_data = query_server_data(host, port, electrum_version)
            server_data = make_json_serializable(server_data)
            
            # Save to Redis
            redis_client.set(host, json.dumps(server_data))
            print(f"Data for server {host} saved to Redis.")
            
        except Exception as e:
            print(f"Error processing server {host}: {e}")

    except Exception as e:
        print(f"Error processing message: {e}")

async def schedule_checks(nc):
    """Periodically fetch server list and schedule checks"""
    while True:
        try:
            servers = fetch_servers()
            if servers:
                current_time = datetime.datetime.utcnow()
                
                for host, details in servers.items():
                    if '.onion' in host:
                        continue  # Skip .onion addresses for now
                    
                    # Check if server was recently checked using Redis
                    if redis_client.exists(host) and not is_stale(host):
                        print(f"Skipping server {host}: recently checked")
                        continue
                        
                    check_data = {
                        'host': host,
                        'port': details.get('s', 50002),
                        'version': details.get('version', 'unknown')
                    }
                    
                    # Publish check request to NATS
                    await nc.publish(NATS_SUBJECT, json.dumps(check_data).encode())
                    print(f"Published check request for {host}")
                    
            else:
                print("No servers found")
                
        except Exception as e:
            print(f"Error in schedule_checks: {e}")
            
        await asyncio.sleep(SERVER_REFRESH_INTERVAL_SECONDS)

async def main():
    # Connect to NATS
    try:
        nc = await nats.connect(NATS_URL)
        print("Connected to NATS successfully!")
        
        # Subscribe to check requests
        sub = await nc.subscribe(NATS_SUBJECT, cb=process_check_request)
        print(f"Subscribed to {NATS_SUBJECT}")
        
        # Start the scheduler
        scheduler = asyncio.create_task(schedule_checks(nc))
        
        # Keep the main task running
        while True:
            await asyncio.sleep(1)
            
    except Exception as e:
        print(f"Error in main: {e}")
        if 'nc' in locals():
            await nc.close()
        exit(1)

if __name__ == "__main__":
    asyncio.run(main())


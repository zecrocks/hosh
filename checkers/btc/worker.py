import redis
import json
import requests
import os
import datetime
import asyncio
import nats
from nats.errors import ConnectionClosedError, TimeoutError, NoServersError

# Environment Variables
BTC_WORKER = os.environ.get('BTC_WORKER', 'http://btc-worker:5000')
NATS_URL = os.environ.get('NATS_URL', 'nats://nats:4222')
NATS_SUBJECT = os.environ.get('NATS_SUBJECT', 'hosh.check.btc')

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
    """Ensure data is JSON serializable, converting datetime objects."""
    for key, value in data.items():
        if isinstance(value, datetime.datetime):
            data[key] = value.isoformat()
    return data


def query_server_data(host, port=50002, electrum_version="unknown"):
    """Query the Electrum server for block header information."""
    url = f"{BTC_WORKER}/electrum/query"
    
    print(f"📡 Sending request to {url} for host {host}")
    
    params = {
        "url": host,
        "method": "blockchain.headers.subscribe",
        "port": port
    }

    try:
        response = requests.get(url, params=params, timeout=30)
        if response.status_code == 400:
            error_data = response.json()
            error_type = error_data.get('error_type', 'unknown')
            
            # Only treat actual connection errors as server failures
            if error_type in ['host_unreachable', 'connection_error', 'protocol_error']:
                print(f"🔴 Server error for {host}: {error_data['error']}")
                return {
                    "host": host,
                    "port": port,
                    "height": 0,
                    "server_version": electrum_version,
                    "LastUpdated": datetime.datetime.utcnow().isoformat(),
                    "error": True,
                    "error_type": error_type,
                    "error_message": error_data['error']
                }
            else:
                # For other errors (like btc-backend timeouts), skip update
                print(f"⚠️ Backend error for {host}: {error_data['error']}")
                return None
                
        response.raise_for_status()
    except requests.Timeout:
        print(f"⏰ Backend timeout while querying {host}")
        return None  # Skip update for backend timeouts
    except requests.RequestException as e:
        print(f"💥 Backend error when querying {host}: {e}")
        return None

    data = response.json()
    data.update({
        "host": host,
        "port": port,
        "electrum_version": electrum_version,
        "LastUpdated": datetime.datetime.utcnow().isoformat(),
        "error": False  # Explicitly mark successful responses
    })
    return data


async def process_check_request(nc, msg):
    """Handle incoming check requests from NATS."""
    try:
        data = json.loads(msg.data.decode())
        host = data['host']
        port = data.get('port', 50002)
        electrum_version = data.get('version', 'unknown')

        print(f"Processing check request for server: {host}")

        server_data = query_server_data(host, port, electrum_version)

        if server_data:  # Only update Redis if data is retrieved successfully
            server_data = make_json_serializable(server_data)
            redis_client.set(f"btc:{host}", json.dumps(server_data))
            print(f"Data for server {host} saved to Redis.")
        else:
            print(f"Skipping Redis update for {host} due to timeout.")

    except Exception as e:
        print(f"Error processing message: {e}")


async def main():
    """Main function to connect to NATS and handle subscriptions."""
    try:
        # Connect to NATS
        nc = await nats.connect(NATS_URL)
        print("Connected to NATS successfully!")

        # Subscribe to `hosh.check`
        async def subscription_handler(msg):
            await process_check_request(nc, msg)

        await nc.subscribe(NATS_SUBJECT, cb=subscription_handler)
        print(f"Subscribed to {NATS_SUBJECT}")

        # Keep the event loop running
        while True:
            await asyncio.sleep(1)

    except Exception as e:
        print(f"Error in main: {e}")

    finally:
        if 'nc' in locals() and nc.is_connected:
            await nc.close()
        print("Disconnected from NATS.")


if __name__ == "__main__":
    asyncio.run(main())

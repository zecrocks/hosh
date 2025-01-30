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
        check_id = data.get('check_id', 'none')
        user_submitted = data.get('user_submitted', False)

        print(f"📥 Received check request - host: {host}, check_id: {check_id}, user_submitted: {user_submitted}")

        server_data = query_server_data(host, port, electrum_version)

        if server_data:
            # Success case - store the data
            server_data.update({
                'user_submitted': user_submitted,
                'check_id': check_id
            })
            server_data = make_json_serializable(server_data)
            redis_client.set(f"btc:{host}", json.dumps(server_data))
            print(f"✅ Data saved to Redis - host: {host}, check_id: {check_id}, user_submitted: {user_submitted}")
        else:
            # Failure case - store error info
            error_data = {
                'host': host,
                'port': port,
                'height': 0,
                'LastUpdated': datetime.datetime.utcnow().isoformat(),
                'error': True,
                'error_type': 'connection_failed',
                'error_message': 'Failed to connect or timeout',
                'user_submitted': user_submitted,
                'check_id': check_id
            }
            redis_client.set(f"btc:{host}", json.dumps(error_data))
            print(f"❌ Check failed - host: {host}, check_id: {check_id}")

    except Exception as e:
        print(f"Error processing message: {e}")


async def main():
    """Main function to connect to NATS and handle subscriptions."""
    try:
        nc = await nats.connect(NATS_URL)
        print("Connected to NATS successfully!")
        print(f"🎯 Subscribing to NATS subject: {NATS_SUBJECT}")

        async def subscription_handler(msg):
            print(f"🔍 Raw message received: {msg.data.decode()}")
            await process_check_request(nc, msg)

        # Add queue group name for load balancing
        queue_group = "btc_checkers"
        await nc.subscribe(NATS_SUBJECT, queue=queue_group, cb=subscription_handler)
        print(f"Subscribed to {NATS_SUBJECT} in queue group '{queue_group}'")

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

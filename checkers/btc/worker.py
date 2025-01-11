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
    for key, value in data.items():
        if isinstance(value, datetime.datetime):
            data[key] = value.isoformat()
    return data

def query_server_data(host, port=50002, electrum_version="unknown"):
    url = f"{BTC_WORKER}/electrum/query"
    params = {
        "url": host,
        "method": "blockchain.headers.subscribe",
        "port": port
    }

    response = requests.get(url, params=params, timeout=10)
    if response.status_code != 200:
        raise Exception(f"HTTP {response.status_code}")

    data = response.json()
    data.update({
        "host": host,
        "port": port,
        "electrum_version": electrum_version,
        "LastUpdated": datetime.datetime.utcnow().isoformat()
    })
    return data

async def process_check_request(msg):
    """Handle incoming check requests from NATS"""
    try:
        data = json.loads(msg.data.decode())
        host = data['host']
        port = data.get('port', 50002)
        electrum_version = data.get('version', 'unknown')

        print(f"Processing check request for server: {host}")
        
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

async def main():
    try:
        nc = await nats.connect(NATS_URL)
        print("Connected to NATS successfully!")
        
        # Subscribe to check requests
        sub = await nc.subscribe(NATS_SUBJECT, cb=process_check_request)
        print(f"Subscribed to {NATS_SUBJECT}")
        
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

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

def fetch_servers():
    try:
        response = requests.get(f"{BTC_WORKER}/electrum/servers", timeout=10)
        servers = response.json().get("servers", {})
        return servers
    except Exception as e:
        print(f"Error fetching servers: {e}")
        return {}

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

async def publish_checks(nc):
    """Periodically fetch server list and publish check requests"""
    while True:
        try:
            servers = fetch_servers()
            if servers:
                current_time = datetime.datetime.utcnow()
                
                for host, details in servers.items():                    
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
            print(f"Error in publish_checks: {e}")
            
        await asyncio.sleep(SERVER_REFRESH_INTERVAL_SECONDS)

async def main():
    try:
        nc = await nats.connect(NATS_URL)
        print("Connected to NATS successfully!")
        
        # Start the publisher
        publisher = asyncio.create_task(publish_checks(nc))
        
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
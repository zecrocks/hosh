import redis
import json
import time
import requests
import os
import datetime
import asyncio

# Environment Variables
DISCOVERY_INTERVAL_SECONDS = int(os.environ.get('DISCOVERY_INTERVAL', 3600))  # 1 hour default

# Redis Configuration
REDIS_HOST = os.environ.get('REDIS_HOST', 'redis')
REDIS_PORT = int(os.environ.get('REDIS_PORT', 6379))

# Static ZEC server configuration
ZEC_SERVERS = [
    {'host': 'zec.rocks', 'port': 443},
    {'host': 'na.zec.rocks', 'port': 443},
    {'host': 'sa.zec.rocks', 'port': 443},
    {'host': 'eu.zec.rocks', 'port': 443},
    {'host': 'ap.zec.rocks', 'port': 443},
    {'host': 'me.zec.rocks', 'port': 443},
    {'host': 'zcashd.zec.rocks', 'port': 443},
    {'host': 'lwd1.zcash-infra.com', 'port': 9067},
    {'host': 'lwd2.zcash-infra.com', 'port': 9067},
    {'host': 'lwd3.zcash-infra.com', 'port': 9067},
    {'host': 'lwd4.zcash-infra.com', 'port': 9067},
    {'host': 'lwd5.zcash-infra.com', 'port': 9067},
    {'host': 'lwd6.zcash-infra.com', 'port': 9067},
    {'host': 'lwd7.zcash-infra.com', 'port': 9067},
    {'host': 'lwd8.zcash-infra.com', 'port': 9067},
]

def fetch_btc_servers():
    try:
        response = requests.get("https://raw.githubusercontent.com/spesmilo/electrum/refs/heads/master/electrum/chains/servers.json", timeout=10)
        return response.json().get("servers", {})
    except Exception as e:
        print(f"Error fetching BTC servers: {e}")
        return {}

async def update_servers(redis_client):
    """Update Redis with known servers"""
    while True:
        try:
            # Update BTC servers
            btc_servers = fetch_btc_servers()
            for host, details in btc_servers.items():
                redis_key = f"btc:{host}"
                if not redis_client.exists(redis_key):
                    server_data = {
                        'host': host,
                        'port': details.get('s', 50002),
                        'version': details.get('version', 'unknown'),
                        'height': 0,
                        'LastUpdated': datetime.datetime.min.isoformat()
                    }
                    redis_client.set(redis_key, json.dumps(server_data))
                    print(f"Added new BTC server: {host}")

            # Update ZEC servers
            for server in ZEC_SERVERS:
                redis_key = f"zec:{server['host']}"
                if not redis_client.exists(redis_key):
                    server_data = {
                        'host': server['host'],
                        'port': server['port'],
                        'height': 0,
                        'LastUpdated': datetime.datetime.min.isoformat()
                    }
                    redis_client.set(redis_key, json.dumps(server_data))
                    print(f"Added new ZEC server: {server['host']}")

        except Exception as e:
            print(f"Error in update_servers: {e}")

        await asyncio.sleep(DISCOVERY_INTERVAL_SECONDS)

async def main():
    try:
        # Connect to Redis
        redis_client = redis.StrictRedis(
            host=REDIS_HOST, 
            port=REDIS_PORT, 
            db=0, 
            socket_timeout=5
        )
        redis_client.ping()
        print("Connected to Redis successfully!")

        # Start the discovery service
        discovery = asyncio.create_task(update_servers(redis_client))
        
        # Keep the main task running
        while True:
            await asyncio.sleep(1)
            
    except Exception as e:
        print(f"Error in main: {e}")
        exit(1)

if __name__ == "__main__":
    asyncio.run(main()) 
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
SERVER_REFRESH_INTERVAL_SECONDS = int(os.environ.get('CHECK_INTERVAL', 300))  # 5 minutes default
NATS_URL = os.environ.get('NATS_URL', 'nats://nats:4222')
NATS_PREFIX = os.environ.get('NATS_PREFIX', 'hosh.')

# Redis Configuration
REDIS_HOST = os.environ.get('REDIS_HOST', 'redis')
REDIS_PORT = int(os.environ.get('REDIS_PORT', 6379))

def is_stale(data):
    try:
        last_updated = data.get("LastUpdated")
        if not last_updated:
            return True

        # Skip if this was a recent user submission
        if data.get("user_submitted", False):
            last_updated_time = datetime.datetime.fromisoformat(last_updated)
            if last_updated_time.tzinfo is not None:
                last_updated_time = last_updated_time.astimezone(datetime.timezone.utc)
            else:
                last_updated_time = last_updated_time.replace(tzinfo=datetime.timezone.utc)

            current_time = datetime.datetime.now(datetime.timezone.utc)
            age = (current_time - last_updated_time).total_seconds()

            # If user submitted and checked within last minute, skip it
            if age < 60:  # or whatever threshold you prefer
                print(f"Skipping recently user-submitted check ({age:.0f}s ago)")
                return False

        # Normal staleness check for regular updates
        last_updated_time = datetime.datetime.fromisoformat(last_updated)
        if last_updated_time.tzinfo is not None:
            last_updated_time = last_updated_time.astimezone(datetime.timezone.utc)
        else:
            last_updated_time = last_updated_time.replace(tzinfo=datetime.timezone.utc)
            
        current_time = datetime.datetime.now(datetime.timezone.utc)
        age = (current_time - last_updated_time).total_seconds()
        return age > SERVER_REFRESH_INTERVAL_SECONDS
    except Exception as e:
        print(f"Error parsing timestamp for data {data}: {e}")
        return True

async def publish_checks(nc, redis_client):
    """Periodically check Redis for servers and publish check requests"""
    while True:
        try:
            # Get all keys from Redis
            for prefix in ['btc:', 'zec:']:
                keys = redis_client.keys(f"{prefix}*")
                if not keys:
                    print(f"No keys found for prefix {prefix}")

                for key in keys:
                    key = key.decode('utf-8')
                    raw_data = redis_client.get(key)
                    if not raw_data:
                        continue

                    try:
                        data = json.loads(raw_data)
                        # Skip if recently checked and not user-submitted
                        if not data.get('user_submitted', False) and not is_stale(data):
                            print(f"Skipping {key}: recently checked")
                            continue

                        # Extract network type and host from key
                        network = key[:3]  # 'btc' or 'zec'
                        host = key[4:]  # everything after 'xxx:'
                        
                        check_data = {
                            'type': network,
                            'host': host,
                            'port': data.get('port', 50002 if network == 'btc' else 9067),
                            'user_submitted': data.get('user_submitted', False),
                            'check_id': data.get('check_id')
                        }

                        # Add version for BTC servers
                        if network == 'btc':
                            check_data['version'] = data.get('version', 'unknown')

                        # Publish check request to NATS
                        subject = f"{NATS_PREFIX}check.{network}"
                        await nc.publish(subject, json.dumps(check_data).encode())
                        print(f"Published {network.upper()} check request for {host}")

                    except json.JSONDecodeError as e:
                        print(f"Error decoding JSON for {key}: {e}")
                    except Exception as e:
                        print(f"Error processing {key}: {e}")

        except Exception as e:
            print(f"Error in publish_checks: {e}")
            
        await asyncio.sleep(SERVER_REFRESH_INTERVAL_SECONDS)

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

        # Connect to NATS
        nc = await nats.connect(NATS_URL)
        print("Connected to NATS successfully!")
        
        # Start the publisher
        publisher = asyncio.create_task(publish_checks(nc, redis_client))
        
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
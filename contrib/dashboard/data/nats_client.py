import os
import json
import asyncio
import nats

# NATS Configuration
NATS_HOST = os.environ.get('NATS_HOST', 'nats')
NATS_PORT = int(os.environ.get('NATS_PORT', 4222))
NATS_URL = f"nats://{NATS_HOST}:{NATS_PORT}"
NATS_PREFIX = os.environ.get('NATS_PREFIX', 'hosh.')  # Match Rust config default


async def publish_http_check_trigger():
    """
    Publish a message to trigger HTTP checks.
    """
    try:
        # Connect to NATS
        nc = await nats.connect(NATS_URL)
        
        # Prepare the message - exactly matching Rust format
        message = {
            "type": "http",
            "host": "trigger",
            "port": 80
        }
        
        # Use same subject format as Rust code
        subject = f"{NATS_PREFIX}check.http"
        
        # Publish the message
        await nc.publish(subject, json.dumps(message).encode())
        print(f"Published HTTP check trigger to NATS subject: {subject}")
        
        # Close NATS connection
        await nc.close()
        return True
        
    except Exception as e:
        print(f"Error triggering HTTP checks: {e}")
        return False


def trigger_http_checks():
    """
    Trigger HTTP checks via NATS.
    """
    try:
        # Run the async function
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        result = loop.run_until_complete(publish_http_check_trigger())
        loop.close()
        return result
    except Exception as e:
        print(f"Error in trigger_http_checks: {e}")
        return False


async def publish_chain_check_trigger(chain_type):
    """
    Publish a message to trigger checks for a specific blockchain.
    
    Args:
        chain_type (str): The chain type, e.g., 'btc' or 'zec'
    """
    try:
        # Connect to NATS
        nc = await nats.connect(NATS_URL)
        
        # Get all keys from Redis for this chain
        import redis
        from .redis_client import redis_client
        
        if not redis_client:
            print(f"Redis client not available")
            return False
            
        keys = [key.decode() for key in redis_client.keys(f'{chain_type}:*')]
        
        if not keys:
            print(f"No {chain_type} servers found in Redis")
            return False
            
        # Publish a check request for each key
        count = 0
        for key in keys:
            # Extract host from key (format is "chain:host")
            host = key.split(':', 1)[1] if ':' in key else key
            
            # Get server data from Redis
            server_data = redis_client.get(key)
            if not server_data:
                continue
                
            try:
                data = json.loads(server_data)
                port = data.get('port', 50002 if chain_type == 'btc' else 9067)
                
                # Create message similar to publisher service
                message = {
                    "host": host,
                    "port": port,
                    "check_id": data.get('check_id'),
                    "user_submitted": data.get('user_submitted', False)
                }
                
                # Use same subject format as in publisher
                subject = f"{NATS_PREFIX}check.{chain_type}"
                
                # Publish the message
                await nc.publish(subject, json.dumps(message).encode())
                count += 1
                
            except json.JSONDecodeError:
                print(f"Invalid JSON for key {key}")
                continue
        
        # Close NATS connection
        await nc.close()
        print(f"Published {count} {chain_type.upper()} check triggers to NATS")
        return count > 0
        
    except Exception as e:
        print(f"Error triggering {chain_type} checks: {e}")
        return False 
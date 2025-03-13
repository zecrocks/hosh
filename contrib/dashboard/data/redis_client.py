import redis
import json
import os
from datetime import datetime, timezone
import time

# Redis Configuration
REDIS_HOST = os.environ.get('REDIS_HOST', 'redis')
REDIS_PORT = int(os.environ.get('REDIS_PORT', 6379))

# Connect to Redis
try:
    redis_client = redis.StrictRedis(
        host=REDIS_HOST, 
        port=REDIS_PORT, 
        db=0, 
        socket_timeout=5,
        decode_responses=True  # Add this line to automatically decode responses
    )
    redis_client.ping()
    print("Connected to Redis successfully!")
except redis.exceptions.ConnectionError as e:
    print(f"Failed to connect to Redis: {e}")
    redis_client = None


def fetch_data_from_redis():
    """
    Fetch server data from Redis.
    """
    if not redis_client:
        return []
        
    try:
        # Get all keys for servers (btc:* and zec:*)
        keys = redis_client.keys('btc:*') + redis_client.keys('zec:*')
        
        # Get data for each key
        server_data = []
        for key in keys:
            data = redis_client.get(key)
            if data:
                try:
                    server = json.loads(data)
                    
                    # Add the key (which contains the network) to the server data
                    network, host = key.split(':', 1)
                    server['network'] = network
                    server['host'] = host
                    
                    # Parse last_updated as datetime if it exists
                    if 'last_updated' in server:
                        try:
                            # Convert ISO format to datetime
                            last_updated = datetime.fromisoformat(
                                server['last_updated'].replace('Z', '+00:00')
                            )
                            server['last_updated'] = last_updated
                        except (ValueError, TypeError):
                            # Keep as is if parsing fails
                            pass
                    
                    server_data.append(server)
                except json.JSONDecodeError:
                    print(f"Error decoding JSON for key {key}")
        
        return server_data
        
    except Exception as e:
        print(f"Error fetching data from Redis: {e}")
        return []


def fetch_blockchain_heights():
    """
    Fetch blockchain heights data from Redis.
    """
    if not redis_client:
        return []
        
    try:
        # Get all http: keys
        keys = redis_client.keys('http:*')
        
        # Group heights by source
        heights = {}
        for key in keys:
            # Parse the key format "http:source.coin"
            parts = key.split('.')
            if len(parts) != 2:
                continue
                
            source = parts[0]  # http:blockchain or http:blockchair
            coin = parts[1]
            
            # Get the height value
            height = redis_client.get(key)
            if height:
                try:
                    height = int(height)  # Convert string to integer
                    
                    # Initialize source dict if needed
                    if source not in heights:
                        heights[source] = {}
                        
                    heights[source][coin] = height
                except (ValueError, TypeError):
                    print(f"Invalid height value for {key}: {height}")
        
        # Convert to records for table display
        records = []
        for source, coins in heights.items():
            source_name = source.replace('http:', '')  # Remove http: prefix but don't capitalize
            for coin, height in coins.items():
                records.append({
                    'Source': source_name,  # Keep original case
                    'Coin': coin,  # Keep original case
                    'Height': height,
                })
        
        # Sort by Source and Coin
        records.sort(key=lambda x: (x['Source'], x['Coin']))
        return records

    except Exception as e:
        print(f"Error fetching blockchain heights from Redis: {e}")
        return []


def format_last_updated(server_data):
    """
    Format the last_updated field to be more readable.
    """
    now = datetime.now(timezone.utc)
    
    for server in server_data:
        if 'last_updated' in server:
            try:
                # Handle both string and datetime objects
                if isinstance(server['last_updated'], str):
                    # Try to parse the timestamp
                    last_updated = datetime.fromisoformat(server['last_updated'].replace('Z', '+00:00'))
                else:
                    last_updated = server['last_updated']
                
                # Calculate time difference
                diff = now - last_updated
                
                # Format based on time difference
                if diff.total_seconds() < 60:
                    formatted = f"{int(diff.total_seconds())} seconds ago"
                elif diff.total_seconds() < 3600:
                    formatted = f"{int(diff.total_seconds() / 60)} minutes ago"
                elif diff.total_seconds() < 86400:
                    formatted = f"{int(diff.total_seconds() / 3600)} hours ago"
                else:
                    formatted = f"{int(diff.total_seconds() / 86400)} days ago"
                
                server['last_updated_formatted'] = formatted
            except (ValueError, TypeError, AttributeError):
                # If we can't parse the timestamp, use the original
                server['last_updated_formatted'] = server['last_updated']
    
    return server_data


def clear_server_data():
    """
    Clear all server data from Redis.
    """
    if not redis_client:
        return False
        
    try:
        # Get all keys for servers
        keys = redis_client.keys('btc:*') + redis_client.keys('zec:*')
        
        # Delete each key
        for key in keys:
            redis_client.delete(key)
            
        return True
    except Exception as e:
        print(f"Error clearing server data: {e}")
        return False


def clear_explorer_data():
    """
    Clear explorer data from Redis.
    """
    if not redis_client:
        return False
        
    try:
        # Delete all keys starting with http:
        for key in redis_client.keys('http:*'):
            redis_client.delete(key)
        print("Explorer data cleared!")
        return True
    except Exception as e:
        print(f"Error clearing explorer data: {e}")
        return False


def get_server_count_by_type():
    """
    Get count of servers by type from Redis.
    """
    if not redis_client:
        return {"btc": 0, "zec": 0, "http": 0}
        
    try:
        btc_count = len(redis_client.keys('btc:*'))
        zec_count = len(redis_client.keys('zec:*'))
        http_count = len(redis_client.keys('http:*'))
        
        return {
            "btc": btc_count,
            "zec": zec_count,
            "http": http_count
        }
    except Exception as e:
        print(f"Error counting servers: {e}")
        return {"btc": 0, "zec": 0, "http": 0} 
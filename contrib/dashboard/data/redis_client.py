import redis
import json
import os
from datetime import datetime, timezone

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
    redis_client = None


def fetch_data_from_redis():
    """
    Fetch server data from Redis and return it as a list of dictionaries.
    """
    if not redis_client:
        return []
        
    try:
        # Fetch all keys from Redis except http:* keys
        keys = [key.decode() for key in redis_client.keys('*') if not key.decode().startswith('http:')]

        if not keys:
            print("No server data found in Redis.")
            return []

        data = []
        for key in keys:
            raw_data = redis_client.get(key)
            if raw_data:
                record = json.loads(raw_data)

                # Convert resolved_ips from a list to a comma-separated string
                if 'resolved_ips' in record and isinstance(record['resolved_ips'], list):
                    record['resolved_ips'] = ", ".join(record['resolved_ips'])

                data.append(record)

        return data

    except Exception as e:
        print(f"Error fetching server data from Redis: {e}")
        return []


def fetch_blockchain_heights():
    """
    Fetch blockchain heights data from Redis.
    """
    if not redis_client:
        return []
        
    try:
        # Get all http: keys
        keys = [key.decode() for key in redis_client.keys('http:*')]
        
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
                height = int(height)
                
                # Initialize source dict if needed
                if source not in heights:
                    heights[source] = {}
                    
                heights[source][coin] = height
        
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
    Format the last_updated field in server data to a human-readable format.
    """
    # Get current time in UTC
    now = datetime.now(timezone.utc)

    # Convert 'last_updated' to time delta
    for record in server_data:
        if 'last_updated' in record:
            last_updated_str = record['last_updated'].strip()
            try:
                last_updated = datetime.fromisoformat(last_updated_str.replace('Z', '+00:00'))
                delta = now - last_updated
                seconds = int(delta.total_seconds())
                
                if seconds < 0:
                    record['last_updated'] = "Never Updated"
                elif seconds < 60:
                    record['last_updated'] = f"{seconds}s ago"
                elif seconds < 3600:
                    record['last_updated'] = f"{seconds // 60}m ago"
                elif seconds < 86400:
                    record['last_updated'] = f"{seconds // 3600}h {(seconds % 3600) // 60}m ago"
                else:
                    record['last_updated'] = f"{seconds // 86400}d {(seconds % 86400) // 3600}h ago"
            except Exception as e:
                print(f"Error parsing last_updated: {e} (Value: {last_updated_str})")
                record['last_updated'] = "Invalid Time"
    
    return server_data


def clear_server_data():
    """
    Clear server data from Redis.
    """
    if not redis_client:
        return False
        
    try:
        # Delete all keys starting with btc: or zec:
        for key in redis_client.keys('*'):
            key_str = key.decode()
            if key_str.startswith(('btc:', 'zec:')):
                redis_client.delete(key)
        print("Server data cleared!")
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
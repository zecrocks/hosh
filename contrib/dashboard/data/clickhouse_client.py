import os
import pandas as pd
from clickhouse_driver import Client
from datetime import datetime, timedelta

# Clickhouse Configuration
CLICKHOUSE_HOST = os.environ.get('CLICKHOUSE_HOST', 'chronicler')
CLICKHOUSE_PORT = int(os.environ.get('CLICKHOUSE_PORT', 9000))
CLICKHOUSE_DB = os.environ.get('CLICKHOUSE_DB', 'hosh')
CLICKHOUSE_USER = os.environ.get('CLICKHOUSE_USER', 'default')
CLICKHOUSE_PASSWORD = os.environ.get('CLICKHOUSE_PASSWORD', '')

# Connect to Clickhouse
try:
    clickhouse_client = Client(
        host=CLICKHOUSE_HOST,
        port=CLICKHOUSE_PORT,
        database=CLICKHOUSE_DB,
        user=CLICKHOUSE_USER,
        password=CLICKHOUSE_PASSWORD
    )
    # Test connection
    result = clickhouse_client.execute("SELECT 1")
    if result[0][0] == 1:
        print("Connected to Clickhouse successfully!")
    else:
        print("Clickhouse connection test failed")
except Exception as e:
    print(f"Failed to connect to Clickhouse: {e}")
    clickhouse_client = None


def fetch_server_stats(time_range='24h'):
    """
    Fetch overall server statistics from Clickhouse.
    
    Args:
        time_range: Time range to fetch data for ('24h', '7d', '30d')
    
    Returns:
        List of dictionaries with server stats
    """
    if not clickhouse_client:
        return []
    
    try:
        # Convert time range to hours
        hours = {
            '24h': 24,
            '7d': 24 * 7,
            '30d': 24 * 30
        }.get(time_range, 24)
        
        # Updated query to match the actual schema
        query = f"""
        SELECT 
            hostname as host,
            '' as port,  -- No direct port column, could extract from hostname if needed
            checker_module as protocol,
            count(*) as total_checks,
            countIf(status = 'online') / count(*) as success_rate,
            avg(ping_ms) as avg_response_time,
            max(checked_at) as last_check
        FROM results
        WHERE checked_at >= now() - INTERVAL {hours} HOUR
        GROUP BY hostname, checker_module
        ORDER BY success_rate DESC, avg_response_time ASC
        """
        
        result = clickhouse_client.execute(query, with_column_types=True)
        
        # Convert to DataFrame
        columns = [col[0] for col in result[1]]
        df = pd.DataFrame(result[0], columns=columns)
        
        # Format for display
        if not df.empty:
            df['success_rate'] = (df['success_rate'] * 100).round(2).astype(str) + '%'
            df['avg_response_time'] = df['avg_response_time'].round(2).astype(str) + ' ms'
            df['last_check'] = pd.to_datetime(df['last_check']).dt.strftime('%Y-%m-%d %H:%M:%S')
            
            # Convert to records for table display
            return df.to_dict('records')
        
        return []
        
    except Exception as e:
        print(f"Error fetching server stats from Clickhouse: {e}")
        return []


def fetch_server_performance(host, port, protocol, time_range='24h'):
    """
    Fetch performance data for a specific server from Clickhouse.
    
    Args:
        host: Server hostname
        port: Server port
        protocol: Server protocol
        time_range: Time range to fetch data for ('24h', '7d', '30d')
    
    Returns:
        DataFrame with server performance data
    """
    if not clickhouse_client:
        return pd.DataFrame()
    
    try:
        # Convert time range to hours
        hours = {
            '24h': 24,
            '7d': 24 * 7,
            '30d': 24 * 30
        }.get(time_range, 24)
        
        # Determine appropriate time interval based on range
        interval = 'toStartOfHour(timestamp)'
        if hours > 72:  # More than 3 days, group by day
            interval = 'toStartOfDay(timestamp)'
        
        # Updated query to match the actual schema
        query = f"""
        SELECT 
            {interval} as time_interval,
            avg(ping_ms) as avg_response_time,
            countIf(status = 'online') / count(*) as success_rate
        FROM results
        WHERE 
            hostname = '{host}' AND 
            checker_module = '{protocol}' AND
            checked_at >= now() - INTERVAL {hours} HOUR
        GROUP BY time_interval
        ORDER BY time_interval
        """
        
        result = clickhouse_client.execute(query, with_column_types=True)
        
        # Convert to DataFrame
        columns = [col[0] for col in result[1]]
        df = pd.DataFrame(result[0], columns=columns)
        
        # Convert timestamp to datetime if it exists
        if 'time_interval' in df.columns:
            df['time_interval'] = pd.to_datetime(df['time_interval'])
            
        return df
        
    except Exception as e:
        print(f"Error fetching server performance from Clickhouse: {e}")
        return pd.DataFrame()


def get_server_list():
    """
    Get a list of all servers in the Clickhouse database.
    
    Returns:
        List of dictionaries with server information
    """
    if not clickhouse_client:
        return []
    
    try:
        # Updated query to match the actual schema
        query = """
        SELECT DISTINCT
            hostname as host,
            '' as port,  -- No direct port column
            checker_module as protocol
        FROM results
        ORDER BY hostname, checker_module
        """
        
        result = clickhouse_client.execute(query)
        
        # Convert to list of dictionaries
        servers = []
        for row in result:
            host, port, protocol = row
            servers.append({
                'label': f"{host} ({protocol})",
                'value': f"{host}:{port}:{protocol}"
            })
            
        return servers
        
    except Exception as e:
        print(f"Error fetching server list from Clickhouse: {e}")
        return [] 
import os
import pandas as pd
from clickhouse_driver import Client
from datetime import datetime, timedelta

# Clickhouse Configuration
CLICKHOUSE_HOST = os.environ.get('CLICKHOUSE_HOST', 'chronicler')
CLICKHOUSE_PORT = int(os.environ.get('CLICKHOUSE_PORT', 8123))
CLICKHOUSE_DB = os.environ.get('CLICKHOUSE_DB', 'hosh')
CLICKHOUSE_USER = os.environ.get('CLICKHOUSE_USER', 'hosh')
CLICKHOUSE_PASSWORD = os.environ.get('CLICKHOUSE_PASSWORD', '')

# Connect to Clickhouse
try:
    clickhouse_client = Client(
        host=CLICKHOUSE_HOST,
        port=CLICKHOUSE_PORT,
        database=CLICKHOUSE_DB,
        user=CLICKHOUSE_USER,
        password=CLICKHOUSE_PASSWORD,
        settings={'use_numpy': False}
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


def get_time_filter(time_range):
    """
    Convert time range string to a SQL filter condition.
    
    Args:
        time_range: String like '10m', '1h', '24h', '7d', '30d'
        
    Returns:
        SQL WHERE clause for the time filter
    """
    now = datetime.now()
    
    if time_range == '10m':
        start_time = now - timedelta(minutes=10)
    elif time_range == '1h':
        start_time = now - timedelta(hours=1)
    elif time_range == '24h':
        start_time = now - timedelta(hours=24)
    elif time_range == '7d':
        start_time = now - timedelta(days=7)
    elif time_range == '30d':
        start_time = now - timedelta(days=30)
    else:
        # Default to 24 hours
        start_time = now - timedelta(hours=24)
    
    return f"checked_at >= toDateTime('{start_time.strftime('%Y-%m-%d %H:%M:%S')}')"


def fetch_server_stats(time_range='24h'):
    """
    Fetch server statistics from ClickHouse.
    
    Args:
        time_range: Time range to filter results (e.g., '10m', '1h', '24h', '7d', '30d')
        
    Returns:
        List of dictionaries with server statistics
    """
    if not clickhouse_client:
        return []
    
    try:
        time_filter = get_time_filter(time_range)
        
        query = f"""
        SELECT
            hostname as host,
            '' as port,  -- No direct port column
            checker_module as protocol,
            round(countIf(status = 'online') / count(*) * 100, 1) as success_rate,
            round(avg(if(status = 'online', ping_ms, null)), 2) as avg_response_time,
            count(*) as total_checks,
            max(checked_at) as last_check
        FROM results
        WHERE {time_filter}
        GROUP BY hostname, checker_module
        ORDER BY hostname, checker_module
        """
        
        result = clickhouse_client.execute(query)
        
        # Convert to list of dictionaries
        stats = []
        for row in result:
            host, port, protocol, success_rate, avg_response_time, total_checks, last_check = row
            
            # Format last_check as a string
            if last_check:
                last_check_str = last_check.strftime('%Y-%m-%d %H:%M:%S')
            else:
                last_check_str = "Never"
                
            stats.append({
                'host': host,
                'port': port,
                'protocol': protocol,
                'success_rate': f"{success_rate}%",
                'avg_response_time': f"{avg_response_time:.2f} ms" if avg_response_time else "N/A",
                'total_checks': total_checks,
                'last_check': last_check_str
            })
            
        return stats
        
    except Exception as e:
        print(f"Error fetching server stats from ClickHouse: {e}")
        return []


def fetch_server_performance(hostname, protocol, time_range='24h'):
    """
    Fetch performance data for a specific server from ClickHouse.
    
    Args:
        hostname: Server hostname
        protocol: Server protocol (e.g., 'btc', 'zec', 'http')
        time_range: Time range to filter results (e.g., '10m', '1h', '24h', '7d', '30d')
        
    Returns:
        List of dictionaries with performance data
    """
    if not clickhouse_client:
        return []
    
    try:
        time_filter = get_time_filter(time_range)
        
        query = f"""
        SELECT
            checked_at,
            status,
            ping_ms
        FROM results
        WHERE hostname = '{hostname}'
          AND checker_module = '{protocol}'
          AND {time_filter}
        ORDER BY checked_at
        """
        
        result = clickhouse_client.execute(query)
        
        # Convert to list of dictionaries
        performance_data = []
        for row in result:
            checked_at, status, ping_ms = row
            performance_data.append({
                'checked_at': checked_at,
                'status': status,
                'ping_ms': ping_ms if ping_ms is not None else 0
            })
            
        return performance_data
        
    except Exception as e:
        print(f"Error fetching server performance from ClickHouse: {e}")
        return []


def get_server_list():
    """
    Get a list of all servers in the Clickhouse database.
    
    Returns:
        List of dictionaries with server information
    """
    if not clickhouse_client:
        return []
    
    try:
        query = """
        SELECT DISTINCT
            hostname as host,
            checker_module as protocol
        FROM results
        ORDER BY hostname, checker_module
        """
        
        result = clickhouse_client.execute(query)
        
        # Convert to list of dictionaries
        servers = []
        for row in result:
            host, protocol = row
            servers.append({
                'label': f"{host} ({protocol})",
                'value': f"{host}::{protocol}"
            })
            
        return servers
        
    except Exception as e:
        print(f"Error fetching server list from Clickhouse: {e}")
        return []


def fetch_targets(time_range='24h'):
    """
    Fetch active targets from ClickHouse.
    """
    if not clickhouse_client:
        return []
    
    try:
        query = """
        SELECT
            hostname,
            module,
            formatDateTime(last_queued_at, '%Y-%m-%d %H:%M:%S') as last_queued_at,
            formatDateTime(last_checked_at, '%Y-%m-%d %H:%M:%S') as last_checked_at,
            user_submitted
        FROM targets
        ORDER BY hostname, module
        """
        
        result = clickhouse_client.execute(query)
        
        # Convert to list of dictionaries
        targets = []
        for row in result:
            hostname, module, last_queued, last_checked, user_submitted = row
            target = {
                'hostname': hostname,
                'module': module,
                'last_queued_at': last_queued,
                'last_checked_at': last_checked,
                'user_submitted': 'Yes' if user_submitted else 'No'
            }
            targets.append(target)
            
        return targets
        
    except Exception as e:
        print(f"Error fetching targets from ClickHouse: {e}")
        return []


def fetch_check_results(hostname, protocol, time_range='24h'):
    """
    Fetch detailed check results for a specific target from ClickHouse.
    
    Args:
        hostname: Server hostname
        protocol: Server protocol (e.g., 'btc', 'zec', 'http')
        time_range: Time range to filter results
        
    Returns:
        List of dictionaries with check results
    """
    if not clickhouse_client:
        return []
    
    try:
        time_filter = get_time_filter(time_range)
        
        query = f"""
        SELECT
            checked_at,
            status,
            ping_ms,
            resolved_ip,
            response_data
        FROM results
        WHERE hostname = '{hostname}'
          AND checker_module = '{protocol}'
          AND {time_filter}
        ORDER BY checked_at DESC
        LIMIT 100
        """
        
        result = clickhouse_client.execute(query)
        
        # Convert to list of dictionaries
        check_results = []
        for row in result:
            checked_at, status, ping_ms, resolved_ip, response_data = row
            check_results.append({
                'checked_at': checked_at.strftime('%Y-%m-%d %H:%M:%S'),
                'status': status,
                'ping_ms': f"{ping_ms:.2f}" if ping_ms is not None else "N/A",
                'resolved_ip': resolved_ip or "N/A",
                'response_data': response_data or "N/A"
            })
            
        return check_results
        
    except Exception as e:
        print(f"Error fetching check results from ClickHouse: {e}")
        return [] 
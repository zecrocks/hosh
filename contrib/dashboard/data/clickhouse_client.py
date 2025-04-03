import os
import pandas as pd
from clickhouse_driver import Client
from datetime import datetime, timedelta
from contextlib import contextmanager
import threading

# Clickhouse Configuration
CLICKHOUSE_HOST = os.environ.get('CLICKHOUSE_HOST', 'chronicler')
CLICKHOUSE_PORT = int(os.environ.get('CLICKHOUSE_PORT', 8123))
CLICKHOUSE_DB = os.environ.get('CLICKHOUSE_DB', 'hosh')
CLICKHOUSE_USER = os.environ.get('CLICKHOUSE_USER', 'hosh')
CLICKHOUSE_PASSWORD = os.environ.get('CLICKHOUSE_PASSWORD')
if not CLICKHOUSE_PASSWORD:
    raise ValueError("CLICKHOUSE_PASSWORD environment variable must be set")

# Connection pool
_connection_pool = []
_connection_lock = threading.Lock()
MAX_CONNECTIONS = 5

def get_connection():
    """Get a connection from the pool or create a new one."""
    with _connection_lock:
        if not _connection_pool:
            client = Client(
                host=CLICKHOUSE_HOST,
                port=CLICKHOUSE_PORT,
                database=CLICKHOUSE_DB,
                user=CLICKHOUSE_USER,
                password=CLICKHOUSE_PASSWORD,
                settings={'use_numpy': False}
            )
            # Test connection
            result = client.execute("SELECT 1")
            if result[0][0] == 1:
                print("Connected to Clickhouse successfully!")
            else:
                print("Clickhouse connection test failed")
            return client
        return _connection_pool.pop()

def release_connection(client):
    """Release a connection back to the pool."""
    with _connection_lock:
        if len(_connection_pool) < MAX_CONNECTIONS:
            _connection_pool.append(client)

@contextmanager
def get_client():
    """Context manager for getting and releasing a ClickHouse client."""
    client = get_connection()
    try:
        yield client
    finally:
        release_connection(client)

# Initialize the connection pool
try:
    for _ in range(MAX_CONNECTIONS):
        client = Client(
            host=CLICKHOUSE_HOST,
            port=CLICKHOUSE_PORT,
            database=CLICKHOUSE_DB,
            user=CLICKHOUSE_USER,
            password=CLICKHOUSE_PASSWORD,
            settings={'use_numpy': False}
        )
        _connection_pool.append(client)
    print(f"Initialized ClickHouse connection pool with {MAX_CONNECTIONS} connections")
except Exception as e:
    print(f"Failed to initialize ClickHouse connection pool: {e}")
    _connection_pool = []

def get_time_filter(time_range):
    """
    Convert time range string to a SQL filter condition.
    
    Args:
        time_range: String like '10m', '1h', '24h', '7d', '30d'
        
    Returns:
        SQL WHERE clause for the time filter
    """
    if time_range == '10m':
        interval = 'INTERVAL 10 MINUTE'
    elif time_range == '1h':
        interval = 'INTERVAL 1 HOUR'
    elif time_range == '24h':
        interval = 'INTERVAL 24 HOUR'
    elif time_range == '7d':
        interval = 'INTERVAL 7 DAY'
    elif time_range == '30d':
        interval = 'INTERVAL 30 DAY'
    else:
        # Default to 24 hours
        interval = 'INTERVAL 24 HOUR'
    
    return f"checked_at >= now() - {interval}"

def fetch_server_stats(time_range='24h'):
    """
    Fetch server statistics from ClickHouse.
    
    Args:
        time_range: Time range to filter results (e.g., '10m', '1h', '24h', '7d', '30d')
        
    Returns:
        List of dictionaries with server statistics
    """
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
        
        with get_client() as client:
            result = client.execute(query)
        
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
    if not _connection_pool:
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
        
        with get_client() as client:
            result = client.execute(query)
        
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
    if not _connection_pool:
        return []
    
    try:
        query = """
        SELECT DISTINCT
            hostname as host,
            checker_module as protocol
        FROM results
        ORDER BY hostname, checker_module
        """
        
        with get_client() as client:
            result = client.execute(query)
        
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
    try:
        query = """
        SELECT
            target_id,
            hostname,
            module,
            formatDateTime(last_queued_at, '%Y-%m-%d %H:%M:%S') as last_queued_at,
            formatDateTime(last_checked_at, '%Y-%m-%d %H:%M:%S') as last_checked_at,
            user_submitted
        FROM targets
        ORDER BY hostname, module
        """
        
        with get_client() as client:
            result = client.execute(query)
        
        # Convert to list of dictionaries
        targets = []
        for row in result:
            target_id, hostname, module, last_queued, last_checked, user_submitted = row
            target = {
                'target_id': str(target_id),  # Convert UUID to string
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
    if not _connection_pool:
        print("No ClickHouse client available")
        return []
    
    try:
        time_filter = get_time_filter(time_range)
        print(f"Using time filter: {time_filter}")
        
        query = f"""
        SELECT
            target_id,
            checked_at,
            hostname,
            resolved_ip,
            ip_version,
            checker_module,
            status,
            ping_ms,
            checker_location,
            checker_id,
            response_data,
            user_submitted
        FROM results
        WHERE hostname = '{hostname}'
          AND checker_module = '{protocol}'
          AND {time_filter}
        ORDER BY checked_at DESC
        LIMIT 100
        """
        print(f"Executing query: {query}")
        
        with get_client() as client:
            result = client.execute(query)
        print(f"Query returned {len(result)} rows")
        
        if result:
            print(f"Sample row: {result[0]}")
        
        # Convert to list of dictionaries
        check_results = []
        for row in result:
            try:
                (target_id, checked_at, hostname, resolved_ip, ip_version, 
                 checker_module, status, ping_ms, checker_location, checker_id, 
                 response_data, user_submitted) = row
                
                formatted_result = {
                    'Target ID': str(target_id),
                    'Checked At': checked_at.strftime('%Y-%m-%d %H:%M:%S'),
                    'Hostname': hostname,
                    'IP Address': resolved_ip or "N/A",
                    'IP Version': ip_version,
                    'Checker Module': checker_module,
                    'Status': status,
                    'Response Time (ms)': f"{ping_ms:.2f}" if ping_ms is not None else "N/A",
                    'Checker Location': checker_location or "N/A",
                    'Checker ID': str(checker_id),
                    'Response Data': response_data or "N/A",
                    'User Submitted': "Yes" if user_submitted else "No"
                }
                check_results.append(formatted_result)
            except Exception as row_error:
                print(f"Error processing row: {row}")
                print(f"Error details: {row_error}")
                continue
            
        print(f"Processed {len(check_results)} results")
        if check_results:
            print(f"Sample processed result: {check_results[0]}")
        
        return check_results
        
    except Exception as e:
        print(f"Error fetching check results from ClickHouse: {e}")
        print(f"Query was: {query}")
        return []

def get_minutes_from_range(time_range):
    """Convert time range string to minutes for ClickHouse query"""
    units = {
        'm': 1,
        'h': 60,
        'd': 1440
    }
    value = int(time_range[:-1])
    unit = time_range[-1].lower()
    return value * units[unit]

def get_targets_and_results_counts():
    """Get counts of targets and results from Clickhouse."""
    try:
        # Get targets count
        targets_query = """
            SELECT count(DISTINCT hostname) as count
            FROM targets
            WHERE module IN ('btc', 'zec')
        """
        
        # Get results count from last hour
        results_query = """
            SELECT count(*) as count
            FROM results
            WHERE checker_module IN ('checker-btc', 'checker-zec')
            AND checked_at >= now() - INTERVAL 1 HOUR
        """
        
        with get_client() as client:
            targets_result = client.execute(targets_query)
            results_result = client.execute(results_query)
            
        targets_count = targets_result[0][0] if targets_result else 0
        results_count = results_result[0][0] if results_result else 0
        
        return {
            "targets": targets_count,
            "results": results_count
        }
    except Exception as e:
        print(f"Error getting targets and results counts from Clickhouse: {e}")
        return {"targets": 0, "results": 0} 
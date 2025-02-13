import dash
from dash import dcc, html, callback_context, dash_table
from dash.dependencies import Input, Output
import redis
import json
import os
from datetime import datetime, timezone
import asyncio
import nats
from dash.long_callback import DiskcacheLongCallbackManager
import diskcache


# Initialize the Dash app
app = dash.Dash(__name__)
app.title = "Electrum Servers Dashboard"

# Redis Configuration
REDIS_HOST = os.environ.get('REDIS_HOST', 'redis')
REDIS_PORT = int(os.environ.get('REDIS_PORT', 6379))

# NATS Configuration
NATS_HOST = os.environ.get('NATS_HOST', 'nats')
NATS_PORT = int(os.environ.get('NATS_PORT', 4222))
NATS_URL = f"nats://{NATS_HOST}:{NATS_PORT}"
NATS_PREFIX = os.environ.get('NATS_PREFIX', 'hosh.')  # Match Rust config default

# Connect to Redis
try:
    redis_client = redis.StrictRedis(host=REDIS_HOST, port=REDIS_PORT, db=0, socket_timeout=5)
    redis_client.ping()
    print("Connected to Redis successfully!")
except redis.exceptions.ConnectionError as e:
    print(f"Failed to connect to Redis: {e}")
    exit(1)


def fetch_data_from_redis():
    """
    Fetch server data from Redis and return it as a list of dictionaries.
    """
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
            source_name = source.replace('http:', '')  # Remove http: prefix
            for coin, height in coins.items():
                records.append({
                    'Source': source_name.capitalize(),
                    'Coin': coin.upper(),
                    'Height': height,
                })
        
        # Sort by Source and Coin
        records.sort(key=lambda x: (x['Source'], x['Coin']))
        return records

    except Exception as e:
        print(f"Error fetching blockchain heights from Redis: {e}")
        return []


### Layout of the App ###
app.layout = html.Div([
    html.H1("Electrum Servers Dashboard", style={'textAlign': 'center'}),
    
    # Controls section
    html.Div([
        html.Div([
            html.Button('Clear Server Data', id='clear-servers-button', n_clicks=0, 
                       style={'backgroundColor': 'red', 'color': 'white', 'marginRight': '10px'}),
            html.Button('Clear Explorer Data', id='clear-explorers-button', n_clicks=0,
                       style={'backgroundColor': 'orange', 'color': 'white', 'marginRight': '10px'}),
            html.Button('Trigger HTTP Checks', id='trigger-http-button', n_clicks=0,
                       style={'backgroundColor': 'green', 'color': 'white'}),
        ], style={'display': 'flex', 'gap': '10px'}),
        html.Div([
            html.Label("Auto-Refresh Interval (seconds):"),
            dcc.Input(id='refresh-interval-input', type='number', value=10, min=1, step=1, 
                     style={'marginLeft': '10px'})
        ], style={'marginTop': '10px', 'marginBottom': '10px'}),
    ], style={'marginBottom': '20px'}),

    # Blockchain Heights section
    html.Div([
        html.H2("Blockchain Heights", style={'marginBottom': '10px'}),
        dash_table.DataTable(
            id='heights-table',
            columns=[
                {'name': 'Source', 'id': 'Source'},
                {'name': 'Coin', 'id': 'Coin'},
                {'name': 'Height', 'id': 'Height', 'type': 'numeric', 'format': {'specifier': ','}},
            ],
            data=[],
            style_table={'overflowX': 'auto'},
            style_cell={'textAlign': 'left', 'padding': '5px'},
            style_header={'fontWeight': 'bold', 'backgroundColor': '#f4f4f4'},
            sort_action='native',
        ),
    ], style={'marginBottom': '30px'}),

    # Server Status section
    html.Div([
        html.H2("Server Status", style={'marginBottom': '10px'}),
        dash_table.DataTable(
            id='servers-table',
            columns=[],
            data=[],
            style_table={'overflowX': 'auto'},
            style_cell={'textAlign': 'left', 'padding': '5px'},
            style_header={'fontWeight': 'bold', 'backgroundColor': '#f4f4f4'},
            sort_action='native',
        ),
    ]),

    dcc.Interval(id='auto-refresh-interval', interval=10000, n_intervals=0)
])


@app.callback(
    Output('auto-refresh-interval', 'interval'),
    Input('refresh-interval-input', 'value')
)
def update_interval(refresh_interval):
    return max(1, refresh_interval or 10) * 1000


@app.callback(
    [Output('servers-table', 'columns'),
     Output('servers-table', 'data'),
     Output('heights-table', 'data')],
    [Input('clear-servers-button', 'n_clicks'),
     Input('clear-explorers-button', 'n_clicks'),
     Input('auto-refresh-interval', 'n_intervals')]
)
def update_tables(clear_servers_clicks, clear_explorers_clicks, auto_refresh_intervals):
    """
    Update both tables based on Redis data.
    """
    ctx = callback_context
    button_id = ctx.triggered[0]['prop_id'].split('.')[0] if ctx.triggered else None

    # Handle clear buttons
    if button_id == 'clear-servers-button':
        try:
            # Delete all keys starting with btc: or zec:
            for key in redis_client.keys('*'):
                key_str = key.decode()
                if key_str.startswith(('btc:', 'zec:')):
                    redis_client.delete(key)
            print("Server data cleared!")
        except Exception as e:
            print(f"Error clearing server data: {e}")
        return [], [], fetch_blockchain_heights()
    elif button_id == 'clear-explorers-button':
        try:
            # Delete all keys starting with http:
            for key in redis_client.keys('http:*'):
                redis_client.delete(key)
            print("Explorer data cleared!")
        except Exception as e:
            print(f"Error clearing explorer data: {e}")
        
        # Get current server data
        server_data = fetch_data_from_redis()
        if not server_data:
            return [], [], []
            
        # Process columns
        sorted_keys = sorted(server_data[0].keys())
        columns = [{"name": key, "id": key} for key in sorted_keys]
        
        return columns, server_data, []

    # Regular update
    server_data = fetch_data_from_redis()
    heights_data = fetch_blockchain_heights()

    if not server_data:
        return [], [], heights_data

    # Process server data
    sorted_keys = sorted(server_data[0].keys())
    columns = [{"name": key, "id": key} for key in sorted_keys]

    # Get current time in UTC
    now = datetime.now(timezone.utc)

    # Convert 'last_updated' to time delta
    for record in data:
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

    # Sort the server data
    sorted_data = sorted(server_data, key=lambda record: record.get("host", "").lower())
    sorted_data = [
        {key: record.get(key, "") for key in sorted_keys} for record in sorted_data
    ]

    return columns, sorted_data, heights_data


@app.long_callback(
    Output('trigger-http-button', 'n_clicks'),
    Input('trigger-http-button', 'n_clicks'),
    manager=DiskcacheLongCallbackManager(diskcache.Cache("./cache"))
)
def trigger_http_checks(n_clicks):
    if not n_clicks:
        return 0
        
    async def publish_message():
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
            
        except Exception as e:
            print(f"Error triggering HTTP checks: {e}")
    
    # Run the async function
    asyncio.run(publish_message())
    return 0


# Run the app
if __name__ == '__main__':
    app.run_server(debug=True, host='0.0.0.0', port=8050)


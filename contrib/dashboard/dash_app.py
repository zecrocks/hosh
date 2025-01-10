import dash
from dash import dcc, html, callback_context, dash_table
from dash.dependencies import Input, Output
import redis
import json
import os

# Initialize the Dash app
app = dash.Dash(__name__)
app.title = "Electrum Servers Dashboard"

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


def fetch_data_from_redis():
    """
    Fetch all data from Redis and return it as a list of dictionaries.
    """
    try:
        # Fetch all keys from Redis
        keys = redis_client.keys('*')

        if not keys:
            print("No data found in Redis.")
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
        print(f"Error fetching data from Redis: {e}")
        return []



### Layout of the App ###
app.layout = html.Div([
    html.H1("Electrum Servers Dashboard", style={'textAlign': 'center'}),
    html.Div([
        html.Button('Clear Redis', id='clear-redis-button', n_clicks=0, style={'backgroundColor': 'red', 'color': 'white'}),
        html.Div([
            html.Label("Auto-Refresh Interval (seconds):"),
            dcc.Input(id='refresh-interval-input', type='number', value=10, min=1, step=1, style={'marginLeft': '10px'})
        ], style={'marginTop': '10px', 'marginBottom': '10px'}),
    ], style={'marginBottom': '20px'}),
    dash_table.DataTable(
        id='data-table',
        columns=[],  # Columns will be dynamically updated
        data=[],  # Data will be dynamically updated
        style_table={'overflowX': 'auto'},
        style_cell={'textAlign': 'left', 'padding': '5px'},
        style_header={'fontWeight': 'bold', 'backgroundColor': '#f4f4f4'},
    ),
    dcc.Interval(id='auto-refresh-interval', interval=10000, n_intervals=0)  # Default 10 seconds
])


@app.callback(
    Output('auto-refresh-interval', 'interval'),
    Input('refresh-interval-input', 'value')
)
def update_interval(refresh_interval):
    """
    Update the auto-refresh interval based on user input.
    """
    # Convert seconds to milliseconds; ensure a minimum of 1 second
    return max(1, refresh_interval or 10) * 1000


@app.callback(
    [Output('data-table', 'columns'),
     Output('data-table', 'data')],
    [Input('clear-redis-button', 'n_clicks'),
     Input('auto-refresh-interval', 'n_intervals')]
)
def update_table(clear_clicks, auto_refresh_intervals):
    """
    Update the table display based on Redis data.
    """
    # Determine which input was triggered
    ctx = callback_context
    button_id = ctx.triggered[0]['prop_id'].split('.')[0] if ctx.triggered else None

    # Handle clear button click
    if button_id == 'clear-redis-button':
        try:
            redis_client.flushdb()  # Clear Redis database
            print("Redis database cleared!")
        except Exception as e:
            print(f"Error clearing Redis: {e}")
        return [], []

    # Fetch data from Redis
    data = fetch_data_from_redis()

    if not data:
        return [], []

    # Sort keys alphabetically for the columns
    sorted_keys = sorted(data[0].keys())  # Sort the keys alphabetically
    columns = [{"name": key, "id": key} for key in sorted_keys]

    # Sort the data records by 'host'
    sorted_data = sorted(data, key=lambda record: record.get("host", "").lower())

    # Reorder the sorted data to match the sorted keys
    sorted_data = [
        {key: record.get(key, "") for key in sorted_keys} for record in sorted_data
    ]

    return columns, sorted_data



# Run the app
if __name__ == '__main__':
    app.run_server(debug=True, host='0.0.0.0', port=8050)


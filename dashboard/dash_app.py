import dash
from dash import dcc, html, dash_table
from dash.dependencies import Input, Output
import requests
import pandas as pd
import os

# Initialize the Dash app
app = dash.Dash(__name__)
app.title = "Electrum Servers Dashboard"

# API URL
BTC_WORKER = os.environ.get('BTC_WORKER', 'http://btc-monitor:5000')

# Fetch server data from the API
def fetch_electrum_servers():
    url = f"{BTC_WORKER}/electrum/servers"
    try:
        print(f"Fetching data from: {url}")  # Debug print
        response = requests.get(url)
        print(f"Response status: {response.status_code}")  # Debug print
        print(f"Response data: {response.text}")  # Debug print

        if response.status_code == 200:
            data = response.json().get("servers", {})
            servers_list = []

            # Process each server
            for host, info in data.items():
                servers_list.append({
                    "Host": host,
                    "SSL Port": info.get("s", "N/A"),
                    "TCP Port": info.get("t", "N/A"),
                    "Pruning": info.get("pruning", "N/A"),
                    "Version": info.get("version", "N/A")
                })

            return pd.DataFrame(servers_list)
        else:
            return pd.DataFrame()  # Return empty DataFrame on error
    except Exception as e:
        print(f"Error fetching servers: {e}")
        return pd.DataFrame()

# Layout of the app
app.layout = html.Div([
    html.H1("Electrum Servers Dashboard", style={'textAlign': 'center'}),
    html.Button('Refresh Data', id='refresh-button', n_clicks=0, style={'marginBottom': '20px'}),
    dash_table.DataTable(
        id='servers-table',
        columns=[
            {"name": "Host", "id": "Host"},
            {"name": "SSL Port", "id": "SSL Port"},
            {"name": "TCP Port", "id": "TCP Port"},
            {"name": "Pruning", "id": "Pruning"},
            {"name": "Version", "id": "Version"}
        ],
        style_table={'overflowX': 'auto'},  # Makes table scrollable horizontally
        style_cell={'textAlign': 'left', 'padding': '5px'},  # Align text in cells
        style_header={'fontWeight': 'bold'},  # Make headers bold
    )
])

# Callback to refresh the table when the button is clicked
@app.callback(
    Output('servers-table', 'data'),
    Input('refresh-button', 'n_clicks')
)
def update_table(n_clicks):
    # Fetch the latest server data
    df = fetch_electrum_servers()
    return df.to_dict('records')  # Update table with new data

# Run the app
if __name__ == '__main__':
    app.run_server(debug=True, host='0.0.0.0', port=8050)


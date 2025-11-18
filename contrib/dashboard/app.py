import dash
import dash_bootstrap_components as dbc
from dash import html, dcc
import atexit
import asyncio
import signal
import sys

# Initialize the app
app = dash.Dash(__name__, 
                external_scriptsheets=[dbc.themes.BOOTSTRAP],
                suppress_callback_exceptions=True,
                use_pages=False)  # We're handling routing manually

# Create the navbar
navbar = dbc.NavbarSimple(
    children=[
        dbc.NavItem(dbc.NavLink("Light Nodes", href="/", id="server-status-link")),
        dbc.NavItem(dbc.NavLink("Explorer Heights", href="/blockchain-heights", id="blockchain-heights-link")),
        dbc.NavItem(dbc.NavLink("ClickHouse Data", href="/clickhouse-data", id="clickhouse-data-link")),
    ],
    brand="Hosh Dashboard",
    brand_href="/",
    color="primary",
    dark=True,
    className="mb-4"
)

# Run the app
if __name__ == '__main__':
    # Run the app
    app.run_server(debug=True, host='0.0.0.0', port=8050) 
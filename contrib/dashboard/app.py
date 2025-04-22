import dash
import dash_bootstrap_components as dbc
from dash import html, dcc
import atexit
import asyncio
import signal
import sys
from data.nats_client import get_nats_client, close_nats_client

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

async def cleanup():
    """Cleanup function to close NATS connection."""
    await close_nats_client()

def signal_handler(sig, frame):
    """Handle shutdown signals"""
    print("Received shutdown signal, cleaning up...")
    loop = asyncio.get_event_loop()
    loop.run_until_complete(cleanup())
    sys.exit(0)

# Register signal handlers
signal.signal(signal.SIGINT, signal_handler)
signal.signal(signal.SIGTERM, signal_handler)

# Register cleanup on exit
atexit.register(lambda: asyncio.run(cleanup()))

# Run the app
if __name__ == '__main__':
    # Initialize event loop
    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)
    
    try:
        # Initialize NATS client
        loop.run_until_complete(get_nats_client())
        
        # Run the app
        app.run_server(debug=True, host='0.0.0.0', port=8050)
    finally:
        # Cleanup
        loop.run_until_complete(cleanup())
        loop.close() 
import dash
import dash_bootstrap_components as dbc
from dash import html, dcc
import atexit
import asyncio
import signal
import sys
from nats.aio.client import Client as NATS
import os

# NATS Configuration
NATS_HOST = os.environ.get('NATS_HOST', 'nats')
NATS_PORT = int(os.environ.get('NATS_PORT', 4222))
NATS_URL = f"nats://{NATS_HOST}:{NATS_PORT}"

# Global event loop and NATS client
loop = None
nats_client = None

async def init_nats():
    """Initialize NATS client"""
    global nats_client
    try:
        nats_client = NATS()
        await nats_client.connect(NATS_URL)
        print(f"Connected to NATS at {NATS_URL}")
        return nats_client
    except Exception as e:
        print(f"Failed to connect to NATS: {e}")
        return None

async def cleanup_nats():
    """Cleanup NATS connection"""
    global nats_client
    if nats_client and nats_client.is_connected:
        try:
            await nats_client.drain()
            await nats_client.close()
            print("NATS connection closed")
        except Exception as e:
            print(f"Error closing NATS connection: {e}")

def signal_handler(sig, frame):
    """Handle shutdown signals"""
    global loop
    print("Received shutdown signal, cleaning up...")
    if loop and loop.is_running():
        loop.create_task(cleanup_nats())
        # Give cleanup tasks a chance to complete
        loop.run_until_complete(asyncio.sleep(1))
    sys.exit(0)

# Initialize the app
app = dash.Dash(__name__, 
                external_stylesheets=[dbc.themes.BOOTSTRAP],
                suppress_callback_exceptions=True,
                use_pages=False)  # We're handling routing manually

# Create the navbar
navbar = dbc.NavbarSimple(
    children=[
        dbc.NavItem(dbc.NavLink("Light Nodes", href="/", id="server-status-link")),
        dbc.NavItem(dbc.NavLink("Explorer Heights", href="/blockchain-heights", id="blockchain-heights-link")),
        dbc.NavItem(dbc.NavLink("ClickHouse Data", href="/clickhouse-data", id="clickhouse-data-link")),
        dbc.NavItem(dbc.NavLink("Check Triggers", href="/check-triggers", id="check-triggers-link")),
    ],
    brand="Hosh Dashboard",
    brand_href="/",
    color="primary",
    dark=True,
    className="mb-4"
)

# Register signal handlers
signal.signal(signal.SIGINT, signal_handler)
signal.signal(signal.SIGTERM, signal_handler)

# Register cleanup on exit
atexit.register(lambda: asyncio.run(cleanup_nats()))

# Run the app
if __name__ == '__main__':
    # Initialize event loop and NATS client
    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)
    
    # Initialize NATS client
    loop.run_until_complete(init_nats())
    
    try:
        app.run_server(debug=True, host='0.0.0.0', port=8050)
    finally:
        if loop and loop.is_running():
            loop.run_until_complete(cleanup_nats())
            loop.close() 
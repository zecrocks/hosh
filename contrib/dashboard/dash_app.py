import dash
from dash import html
import dash_bootstrap_components as dbc
from dash.long_callback import DiskcacheLongCallbackManager
import diskcache

# Import layouts
from layouts.main_layout import create_layout

# Import callbacks
from callbacks.navigation import register_callbacks as register_navigation_callbacks
from callbacks.server_status import register_callbacks as register_server_status_callbacks
from callbacks.blockchain_heights import register_callbacks as register_blockchain_heights_callbacks
from callbacks.clickhouse_data import register_callbacks as register_clickhouse_data_callbacks

# Initialize the Diskcache for long callbacks
cache = diskcache.Cache("./cache")
long_callback_manager = DiskcacheLongCallbackManager(cache)

# Initialize the Dash app with Bootstrap
app = dash.Dash(
    __name__, 
    external_stylesheets=[dbc.themes.BOOTSTRAP],
    long_callback_manager=long_callback_manager,
    suppress_callback_exceptions=True
)
app.title = "Electrum Servers Dashboard"

# Set the app layout
app.layout = create_layout()

# Register callbacks
register_navigation_callbacks(app)
register_server_status_callbacks(app, long_callback_manager)
register_blockchain_heights_callbacks(app)
register_clickhouse_data_callbacks(app)

# Run the app
if __name__ == '__main__':
    app.run_server(debug=True, host='0.0.0.0', port=8050)


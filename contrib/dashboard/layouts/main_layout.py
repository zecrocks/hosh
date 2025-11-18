from dash import html, dcc
import dash_bootstrap_components as dbc
from layouts.clickhouse_data import create_layout as create_clickhouse_data_layout

def create_navbar():
    """
    Create the navigation bar.
    """
    return dbc.NavbarSimple(
        children=[
            dbc.NavItem(dbc.NavLink("Clickhouse Data", href="/clickhouse-data", id="clickhouse-data-link")),
        ],
        brand="Lightwallet Servers Dashboard",
        brand_href="/",
        color="primary",
        dark=True,
    )

def create_layout():
    """
    Create the main layout.
    """
    # Add dcc.Location to track URL changes
    return html.Div([
        dcc.Location(id='url', refresh=False),
        create_navbar(),
        html.Div([
            html.Div([
                html.Label("Auto-refresh interval (seconds): ", className="me-2"),
                dcc.Input(
                    id='refresh-interval-input',
                    type='number',
                    min=1,
                    max=300,
                    value=60,
                    className="form-control form-control-sm d-inline-block",
                    style={"width": "80px"}
                ),
            ], className="mb-3 mt-3"),
            
            # Interval component for auto-refresh
            dcc.Interval(
                id='auto-refresh-interval',
                interval=60 * 1000,
                n_intervals=0
            ),
            
            # Store the current page
            dcc.Store(id='current-page', data='clickhouse-data'),
            
            # Main content area
            html.Div(id='page-content', children=create_clickhouse_data_layout())
        ], className="container mt-4")
    ]) 
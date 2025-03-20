from dash import html, dcc
import dash_bootstrap_components as dbc
from layouts.server_status import create_layout as create_server_status_layout

def create_navbar():
    """
    Create the navigation bar.
    """
    return dbc.NavbarSimple(
        children=[
            dbc.NavItem(dbc.NavLink("Nodes Status", href="/", id="server-status-link")),
            dbc.NavItem(dbc.NavLink("Explorer Heights", href="/blockchain-heights", id="blockchain-heights-link")),
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
                    value=10,
                    className="form-control form-control-sm d-inline-block",
                    style={"width": "80px"}
                ),
            ], className="mb-3 mt-3"),
            
            # Interval component for auto-refresh
            dcc.Interval(
                id='auto-refresh-interval',
                interval=10 * 1000,  # in milliseconds
                n_intervals=0
            ),
            
            # Store the current page
            dcc.Store(id='current-page', data='server-status'),
            
            # Main content area
            html.Div(id='page-content', children=create_server_status_layout())
        ], className="container mt-4")
    ]) 
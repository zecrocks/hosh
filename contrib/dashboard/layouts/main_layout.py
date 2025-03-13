from dash import html, dcc
import dash_bootstrap_components as dbc
from layouts.server_status import create_layout as create_server_status_layout

def create_navbar():
    """
    Create the navigation bar.
    """
    return dbc.NavbarSimple(
        children=[
            dbc.NavItem(dbc.NavLink("Nodes Status", href="#", id="server-status-link")),
            dbc.NavItem(dbc.NavLink("Explorer Heights", href="#", id="blockchain-heights-link")),
            dbc.NavItem(dbc.NavLink("Clickhouse Data", href="#", id="clickhouse-data-link")),
        ],
        brand="Lightwallet Servers Dashboard",
        brand_href="#",
        color="primary",
        dark=True,
    )

def create_layout():
    """
    Create the main app layout.
    """
    # Default to server status page
    content = html.Div(id="page-content", children=create_server_status_layout())
    
    return html.Div([
        create_navbar(),
        dbc.Container([
            html.Div([
                html.Label("Auto-Refresh Interval (seconds):"),
                dcc.Input(id='refresh-interval-input', type='number', value=10, min=1, step=1, 
                         className='ms-2')
            ], className='mt-3 mb-3'),
            
            content,
            
            dcc.Interval(id='auto-refresh-interval', interval=10000, n_intervals=0),
            dcc.Store(id='current-page', data='server-status'),
        ], className='mt-4')
    ]) 
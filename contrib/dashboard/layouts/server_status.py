from dash import html, dash_table
import dash_bootstrap_components as dbc
from data.redis_client import get_server_count_by_type

def create_layout():
    """
    Create the light nodes page layout.
    """
    # Get server counts for the button labels
    server_counts = get_server_count_by_type()
    
    return html.Div([
        html.Div([
            html.Button('Clear Server Data', id='clear-servers-button', n_clicks=0,
                       className='btn btn-warning me-2'),
            
            # Add check trigger buttons
            dbc.Button(
                f"Trigger BTC Checks ({server_counts['btc']})", 
                id="trigger-btc-button", 
                color="primary",
                className="me-2"
            ),
            dbc.Button(
                f"Trigger ZEC Checks ({server_counts['zec']})", 
                id="trigger-zec-button", 
                color="primary",
                className="me-2"
            ),
            dbc.Button(
                f"Trigger HTTP Checks ({server_counts['http']})", 
                id="trigger-http-button", 
                color="primary"
            ),
        ], className="mb-3 d-flex flex-wrap gap-2"),
        
        # Results area for check triggers
        html.Div([
            html.Div(id="btc-trigger-result", className="me-2"),
            html.Div(id="zec-trigger-result", className="me-2"),
            html.Div(id="http-trigger-result")
        ], className="mb-3 d-flex flex-wrap gap-2"),
        
        html.Div([
            html.H2("Light Nodes", className='mb-3'),
            dash_table.DataTable(
                id='servers-table',
                columns=[],
                data=[],
                style_table={'overflowX': 'auto'},
                style_cell={'textAlign': 'left', 'padding': '5px'},
                style_header={'fontWeight': 'bold', 'backgroundColor': '#f4f4f4'},
                sort_action='native',
                filter_action='native',
                page_action='native',
                page_size=20,
            ),
        ]),
    ]) 
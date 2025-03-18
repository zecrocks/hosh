from dash import html, dash_table
import dash_bootstrap_components as dbc
from data.redis_client import get_server_count_by_type

def create_layout():
    """
    Create the explorer heights page layout.
    """
    # Get server counts for the button labels
    server_counts = get_server_count_by_type()
    
    return html.Div([
        html.Div([
            html.Button('Clear Explorer Data', id='clear-explorers-button', n_clicks=0,
                       className='btn btn-warning me-2'),
            
            # Add HTTP check trigger button
            dbc.Button(
                f"Trigger HTTP Checks ({server_counts['http']})", 
                id="trigger-http-button", 
                color="primary"
            ),
        ]),
        
        # Add result area for HTTP checks
        html.Div([
            html.Div(id="http-trigger-result", className="mt-2")
        ]),
        
        html.Div([
            html.H2("Explorer Heights", className='mb-3'),
            dash_table.DataTable(
                id='heights-table',
                columns=[
                    {'name': 'Source', 'id': 'Source'},
                    {'name': 'Coin', 'id': 'Coin'},
                    {'name': 'Height', 'id': 'Height', 'type': 'numeric', 'format': {'specifier': ','}},
                ],
                data=[],
                style_table={'overflowX': 'auto'},
                style_cell={'textAlign': 'left', 'padding': '5px'},
                style_header={'fontWeight': 'bold', 'backgroundColor': '#f4f4f4'},
                sort_action='native',
                # The column order will be set dynamically in the callback
            ),
        ]),
    ]) 
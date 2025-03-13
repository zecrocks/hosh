from dash import html, dash_table
import dash_bootstrap_components as dbc

def create_layout():
    """
    Create the explorer heights page layout.
    """
    return html.Div([
        html.Div([
            html.Button('Clear Explorer Data', id='clear-explorers-button', n_clicks=0,
                       className='btn btn-warning mb-3'),
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
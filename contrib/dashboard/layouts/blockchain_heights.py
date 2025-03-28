from dash import html, dcc, dash_table
import dash_bootstrap_components as dbc

# Define block explorer URLs
BLOCK_EXPLORERS = [
    {"label": "Blockchair.com", "value": "https://blockchair.com"},
    {"label": "Blockchair.onion", "value": "http://blkchairbknpn73cfjhevhla7rkp4ed5gg2knctvv7it4lioy22defid.onion"},
    {"label": "Blockstream.info", "value": "https://blockstream.info"},
    {"label": "Zec.rocks", "value": "https://explorer.zec.rocks"},
    {"label": "Blockchain.com", "value": "https://blockchain.com"},
    {"label": "Zcash Explorer", "value": "https://mainnet.zcashexplorer.app"},
    {"label": "Mempool.space", "value": "https://mempool.space"}
]

def create_layout():
    """
    Create the explorer heights page layout.
    """
    return html.Div([
        html.Div([
            # Add HTTP check trigger section with URL dropdown
            html.Div([
                html.Div([
                    html.Label("Select Explorer URL:", className="me-2"),
                    dcc.Dropdown(
                        id="explorer-dropdown",
                        options=BLOCK_EXPLORERS,
                        value=BLOCK_EXPLORERS[0]["value"],
                        className="d-inline-block",
                        style={"width": "400px", "marginRight": "10px"}
                    ),
                ], className="mb-2"),
                html.Div([
                    dbc.Button(
                        "Trigger HTTP Check", 
                        id="trigger-http-button", 
                        color="primary",
                        className="me-2"
                    ),
                    dbc.Switch(
                        id="http-dry-run-toggle",
                        label="Dry Run",
                        value=False,
                        className="d-inline-block"
                    )
                ])
            ], className="mt-3 mb-3"),
        ]),
        
        # Add result area for HTTP checks
        html.Div([
            html.Div(id="http-trigger-result", className="mt-2")
        ]),

        # Add block explorer results table
        html.Div([
            html.H2("Block Explorer Results", className='mb-3'),
            dash_table.DataTable(
                id='explorer-heights-table',
                columns=[
                    {'name': 'Explorer', 'id': 'explorer', 'type': 'text'},
                    {'name': 'Chain', 'id': 'chain', 'type': 'text'},
                    {'name': 'Block Height', 'id': 'block_height', 'type': 'numeric', 'format': {'specifier': ','}},
                    {'name': 'Response Time (ms)', 'id': 'response_time_ms', 'type': 'numeric', 'format': {'specifier': '.1f'}},
                    {'name': 'Checked At', 'id': 'checked_at', 'type': 'text'},
                    {'name': 'Error', 'id': 'error', 'type': 'text'}
                ],
                data=[],
                style_table={'overflowX': 'auto'},
                style_cell={'textAlign': 'left', 'padding': '5px'},
                style_header={'fontWeight': 'bold', 'backgroundColor': '#f4f4f4'},
                sort_action='native',
                sort_mode='multi',
                page_size=20,
                filter_action='native',
            ),
        ], className="mt-4"),
    ]) 
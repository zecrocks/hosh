from dash import html, dcc
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
    ]) 
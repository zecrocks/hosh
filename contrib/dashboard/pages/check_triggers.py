import dash
from dash import html, dcc, callback, Input, Output, State
import dash_bootstrap_components as dbc
import asyncio
from data.nats_client import publish_http_check_trigger, publish_chain_check_trigger

def register_page(app):
    dash.register_page(__name__, path='/check-triggers', name='Check Triggers')

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

layout = html.Div([
    html.H1("Blockchain Check Triggers"),
    html.P("Manually trigger checks for different blockchain networks"),
    
    html.Div([
        dbc.Card([
            dbc.CardHeader("Bitcoin (BTC) Checks"),
            dbc.CardBody([
                html.P("Trigger checks for all Bitcoin servers in the database"),
                dbc.Button("Trigger BTC Checks", id="trigger-btc-button", color="primary"),
                html.Div(id="btc-trigger-result", className="mt-3")
            ])
        ], className="mb-4"),
        
        dbc.Card([
            dbc.CardHeader("Zcash (ZEC) Checks"),
            dbc.CardBody([
                html.P("Trigger checks for all Zcash servers in the database"),
                dbc.Button("Trigger ZEC Checks", id="trigger-zec-button", color="primary"),
                html.Div(id="zec-trigger-result", className="mt-3")
            ])
        ], className="mb-4"),
        
        dbc.Card([
            dbc.CardHeader("HTTP Explorer Checks"),
            dbc.CardBody([
                html.P("Select a block explorer to check:"),
                dcc.Dropdown(
                    id="explorer-dropdown",
                    options=BLOCK_EXPLORERS,
                    value=BLOCK_EXPLORERS[0]["value"],  # Default to first option
                    className="mb-3"
                ),
                dbc.Button("Trigger HTTP Check", id="trigger-http-button", color="primary"),
                html.Div(id="http-trigger-result", className="mt-3")
            ])
        ], className="mb-4"),
    ])
])

@callback(
    Output("btc-trigger-result", "children"),
    Input("trigger-btc-button", "n_clicks"),
    prevent_initial_call=True
)
def trigger_btc_checks(n_clicks):
    if n_clicks:
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        success = loop.run_until_complete(publish_chain_check_trigger('btc'))
        loop.close()
        
        if success:
            return html.Div("✅ BTC checks triggered successfully!", className="text-success")
        else:
            return html.Div("❌ Failed to trigger BTC checks. See console for details.", className="text-danger")
    return ""

@callback(
    Output("zec-trigger-result", "children"),
    Input("trigger-zec-button", "n_clicks"),
    prevent_initial_call=True
)
def trigger_zec_checks(n_clicks):
    if n_clicks:
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        success = loop.run_until_complete(publish_chain_check_trigger('zec'))
        loop.close()
        
        if success:
            return html.Div("✅ ZEC checks triggered successfully!", className="text-success")
        else:
            return html.Div("❌ Failed to trigger ZEC checks. See console for details.", className="text-danger")
    return ""

@callback(
    Output("http-trigger-result", "children"),
    [Input("trigger-http-button", "n_clicks")],
    [State("explorer-dropdown", "value")],
    prevent_initial_call=True
)
def trigger_http_checks(n_clicks, selected_url):
    if n_clicks:
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        success = loop.run_until_complete(publish_http_check_trigger(url=selected_url))
        loop.close()
        
        if success:
            return html.Div(f"✅ HTTP check triggered successfully for {selected_url}!", className="text-success")
        else:
            return html.Div("❌ Failed to trigger HTTP check. See console for details.", className="text-danger")
    return "" 
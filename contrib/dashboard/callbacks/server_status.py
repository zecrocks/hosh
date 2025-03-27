from dash.dependencies import Input, Output, State
from dash import callback_context, html
from dash.long_callback import DiskcacheLongCallbackManager
import diskcache
import asyncio
from data.nats_client import publish_http_check_trigger, publish_chain_check_trigger

def register_callbacks(app, long_callback_manager):
    """
    Register server status callbacks.
    """
    @app.callback(
        [Output('servers-table', 'columns'),
         Output('servers-table', 'data')],
        [Input('clear-servers-button', 'n_clicks'),
         Input('auto-refresh-interval', 'n_intervals'),
         Input('current-page', 'data'),
         Input('network-filter', 'value')]
    )
    def update_servers_table(clear_servers_clicks, auto_refresh_intervals, current_page, network_filter):
        """
        Update the servers table based on Redis data.
        Sort columns based on the number of non-empty values.
        """
        # Temporarily disabled Redis-based table update
        return [], []

    @app.callback(
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
                return html.Div("✅ BTC checks triggered", className="text-success")
            else:
                return html.Div("❌ Failed to trigger BTC checks", className="text-danger")
        return ""

    @app.callback(
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
                return html.Div("✅ ZEC checks triggered", className="text-success")
            else:
                return html.Div("❌ Failed to trigger ZEC checks", className="text-danger")
        return ""

    @app.callback(
        Output("server-status-http-trigger-result", "children"),
        Input("trigger-http-button", "n_clicks"),
        prevent_initial_call=True
    )
    def trigger_http_checks(n_clicks):
        if n_clicks:
            from data.nats_client import trigger_http_checks
            success = trigger_http_checks()
            
            if success:
                return html.Div("✅ HTTP checks triggered", className="text-success")
            else:
                return html.Div("❌ Failed to trigger HTTP checks", className="text-danger")
        return "" 
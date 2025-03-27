from dash.dependencies import Input, Output, State
from dash import callback_context, html, no_update
from data.redis_client import clear_explorer_data
from data.nats_client import publish_http_check_trigger
import asyncio

def register_callbacks(app):
    """
    Register blockchain heights callbacks.
    """
    @app.callback(
        Output("http-trigger-result", "children"),
        Input("trigger-http-button", "n_clicks"),
        [State("explorer-dropdown", "value"),
         State("http-dry-run-toggle", "value")],
        prevent_initial_call=True
    )
    def handle_http_checks_trigger(n_clicks, selected_url, dry_run):
        if n_clicks is None:
            return no_update
            
        try:
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            success = loop.run_until_complete(publish_http_check_trigger(url=selected_url, dry_run=dry_run))
            loop.close()
            
            if success:
                return html.Div(f"✅ HTTP check triggered successfully for {selected_url}! {'(Dry Run)' if dry_run else ''}", className="text-success")
            else:
                return html.Div("❌ Failed to trigger checks", className="text-danger")
        except Exception as e:
            return html.Div(f"❌ Failed to trigger checks: {str(e)}", className="text-danger") 
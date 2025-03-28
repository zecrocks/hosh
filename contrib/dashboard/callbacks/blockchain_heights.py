from dash.dependencies import Input, Output, State
from dash import callback_context, html, no_update
from data.nats_client import publish_http_check_trigger
from data.clickhouse_client import clickhouse_client
import asyncio
import os
from datetime import datetime, timezone

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
                return html.Div("❌ Failed to trigger HTTP check", className="text-danger")
        except Exception as e:
            return html.Div(f"❌ Error triggering HTTP check: {str(e)}", className="text-danger")

    @app.callback(
        Output("explorer-heights-table", "data"),
        [Input("trigger-http-button", "n_clicks"),
         Input("auto-refresh-interval", "n_intervals")],
        prevent_initial_call=False
    )
    def update_explorer_heights_table(n_clicks, n_intervals):
        if not clickhouse_client:
            return []
            
        try:
            # Query to get the latest results for each explorer/chain combination
            query = """
            SELECT
                explorer,
                chain,
                block_height,
                response_time_ms,
                checked_at,
                error
            FROM block_explorer_heights
            WHERE (explorer, chain, checked_at) IN (
                SELECT explorer, chain, max(checked_at)
                FROM block_explorer_heights
                GROUP BY explorer, chain
            )
            ORDER BY checked_at DESC
            """
            
            result = clickhouse_client.execute(query)
            
            # Convert results to list of dictionaries for the table
            table_data = []
            for row in result:
                explorer, chain, block_height, response_time_ms, checked_at, error = row
                
                # Convert timestamp to UTC string
                checked_at_str = checked_at.astimezone(timezone.utc).strftime('%Y-%m-%d %H:%M:%S UTC')
                
                table_data.append({
                    'explorer': explorer,
                    'chain': chain,
                    'block_height': block_height,
                    'response_time_ms': response_time_ms,
                    'checked_at': checked_at_str,
                    'error': error if error else ''
                })
            
            return table_data
            
        except Exception as e:
            print(f"Error fetching explorer heights from ClickHouse: {e}")
            return [] 
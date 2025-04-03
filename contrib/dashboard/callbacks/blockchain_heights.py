from dash.dependencies import Input, Output, State
from dash import callback_context, html, no_update
from data.nats_client import publish_http_check_trigger
from data.clickhouse_client import get_client
import asyncio
import os
from datetime import datetime, timezone
from layouts.blockchain_heights import get_http_targets

# Create a single event loop for all async operations
loop = asyncio.new_event_loop()
asyncio.set_event_loop(loop)

def register_callbacks(app):
    """
    Register blockchain heights callbacks.
    """
    @app.callback(
        Output("http-target-dropdown", "options"),
        [Input("auto-refresh-interval", "n_intervals")],
        prevent_initial_call=False
    )
    def update_http_targets(n_intervals):
        """Update the HTTP targets dropdown with current data from ClickHouse."""
        return get_http_targets()

    @app.callback(
        Output("http-trigger-result", "children"),
        Input("trigger-http-check-button", "n_clicks"),
        [State("http-target-dropdown", "value"),
         State("dry-run-switch", "value")],
        prevent_initial_call=True
    )
    def handle_http_checks_trigger(n_clicks, selected_url, dry_run):
        print(f"HTTP check trigger callback called with n_clicks={n_clicks}, url={selected_url}, dry_run={dry_run}")
        
        if n_clicks is None:
            print("No click detected, returning no_update")
            return no_update
            
        try:
            print(f"Calling publish_http_check_trigger with url={selected_url}, dry_run={dry_run}")
            success = loop.run_until_complete(publish_http_check_trigger(url=selected_url, dry_run=dry_run))
            
            print(f"publish_http_check_trigger returned success={success}")
            
            if success:
                return html.Div(f"✅ HTTP check triggered successfully for {selected_url}! {'(Dry Run)' if dry_run else ''}", className="text-success")
            else:
                return html.Div("❌ Failed to trigger HTTP check", className="text-danger")
        except Exception as e:
            print(f"Exception in handle_http_checks_trigger: {str(e)}")
            import traceback
            print("Full traceback:", traceback.format_exc())
            return html.Div(f"❌ Error triggering HTTP check: {str(e)}", className="text-danger")

    @app.callback(
        Output("explorer-heights-table", "data"),
        [Input("trigger-http-check-button", "n_clicks"),
         Input("auto-refresh-interval", "n_intervals")],
        prevent_initial_call=False
    )
    def update_explorer_heights_table(n_clicks, n_intervals):
        if not get_client():
            print("No ClickHouse client available")
            return []
            
        try:
            # Simplified query to get the latest results for each explorer/chain combination
            query = """
            SELECT
                explorer,
                chain,
                checked_at as last_check,
                block_height as last_height,
                response_time_ms as response_time
            FROM block_explorer_heights
            WHERE (explorer, chain, checked_at) IN (
                SELECT explorer, chain, max(checked_at)
                FROM block_explorer_heights
                GROUP BY explorer, chain
            )
            ORDER BY checked_at DESC
            """
            
            print("Executing query:", query)
            with get_client() as client:
                result = client.execute(query)
            
            print(f"Query returned {len(result)} rows")
            
            # Convert results to list of dictionaries for the table
            table_data = []
            for row in result:
                explorer, chain, last_check, last_height, response_time = row
                
                # Convert timestamp to UTC string
                last_check_str = last_check.strftime('%Y-%m-%d %H:%M:%S UTC') if last_check else 'Never'
                
                table_data.append({
                    'hostname': f"{explorer}.{chain}",
                    'module': 'http',
                    'last_check': last_check_str,
                    'last_height': last_height or 'N/A',
                    'response_time': f"{response_time:.1f}ms" if response_time else 'N/A'
                })
            
            print(f"Processed {len(table_data)} rows for display")
            return table_data
            
        except Exception as e:
            print(f"Error fetching explorer heights from ClickHouse: {e}")
            import traceback
            print("Full traceback:", traceback.format_exc())
            return []

def get_explorer_heights():
    """Get current block heights from Clickhouse."""
    try:
        query = """
        SELECT
            hostname,
            checker_module,
            max(checked_at) as last_check,
            max(if(status = 'online', response_data, null)) as last_height
        FROM results
        WHERE checker_module = 'http'
        GROUP BY hostname, checker_module
        ORDER BY hostname
        """
        
        with get_client() as client:
            result = client.execute(query)
            
        heights = []
        for row in result:
            hostname, module, last_check, last_height = row
            heights.append({
                'hostname': hostname,
                'module': module,
                'last_check': last_check.strftime('%Y-%m-%d %H:%M:%S') if last_check else 'Never',
                'last_height': last_height or 'N/A'
            })
        return heights
        
    except Exception as e:
        print(f"Error fetching explorer heights from ClickHouse: {e}")
        return [] 
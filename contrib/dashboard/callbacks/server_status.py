from dash.dependencies import Input, Output, State
from dash import callback_context, html
from dash.long_callback import DiskcacheLongCallbackManager
import diskcache
import asyncio
from data.redis_client import fetch_data_from_redis, format_last_updated, clear_server_data
from data.nats_client import publish_http_check_trigger, publish_chain_check_trigger
import json

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
        print(f"DEBUG: Callback triggered with current_page={current_page}, network_filter={network_filter}")
        
        # Only update if we're on the light nodes page
        if current_page != 'server-status':
            print("DEBUG: Not on server-status page, returning empty lists")
            return [], []
        
        ctx = callback_context
        button_id = ctx.triggered[0]['prop_id'].split('.')[0] if ctx.triggered else None
        print(f"DEBUG: Triggered by button_id={button_id}")

        # Handle clear button
        if button_id == 'clear-servers-button':
            print("DEBUG: Clear button pressed, clearing data")
            clear_server_data()
            return [], []

        # Regular update
        server_data = fetch_data_from_redis()
        print(f"DEBUG: Fetched {len(server_data)} records from Redis")

        if not server_data:
            print("DEBUG: No server data returned from Redis")
            return [], []

        # Filter data based on selected network
        if network_filter != 'all':
            filtered_data = [server for server in server_data if server.get('network') == network_filter]
            print(f"DEBUG: Filtered data for network '{network_filter}': {len(filtered_data)} records (from {len(server_data)})")
            server_data = filtered_data

        # Format last_updated field
        server_data = format_last_updated(server_data)
        print("DEBUG: Formatted last_updated fields")
        
        # Process data to ensure all values are strings, numbers, or booleans
        processed_data = []
        json_columns = set()
        
        for record in server_data:
            processed_record = {}
            for key, value in record.items():
                # Convert complex data types to JSON strings
                if isinstance(value, (dict, list, tuple)):
                    json_columns.add(key)
                    if isinstance(value, dict):
                        processed_record[key] = f"JSON object ({len(value)} keys)"
                    elif isinstance(value, (list, tuple)):
                        processed_record[key] = f"JSON array ({len(value)} items)"
                    processed_record[f"{key}_full"] = json.dumps(value)
                elif value is None:
                    processed_record[key] = ""
                else:
                    processed_record[key] = value
            processed_data.append(processed_record)
        
        print(f"DEBUG: Processed {len(processed_data)} records, found {len(json_columns)} JSON columns")
        
        # Sort the server data by host
        sorted_data = sorted(processed_data, key=lambda record: str(record.get("host", "")).lower())
        
        # Count non-empty values in each column
        column_counts = {}
        all_keys = set()
        
        # First collect all possible keys, excluding the hidden _full fields
        for record in processed_data:
            visible_keys = [k for k in record.keys() if not k.endswith('_full')]
            all_keys.update(visible_keys)
        
        print(f"DEBUG: Found {len(all_keys)} unique columns")
        
        # Sort keys alphabetically instead of by count
        sorted_keys = sorted(all_keys)
        
        # Create columns list in alphabetical order
        columns = []
        for key in sorted_keys:
            if key in json_columns:
                columns.append({
                    "name": key, 
                    "id": key,
                    "type": "text",
                    "presentation": "markdown"
                })
            else:
                columns.append({"name": key, "id": key})
        
        print(f"DEBUG: Created {len(columns)} columns")
        
        # Format data with sorted keys
        formatted_data = []
        for record in sorted_data:
            formatted_record = {}
            for key in sorted_keys:
                formatted_record[key] = record.get(key, "")
            formatted_data.append(formatted_record)

        print(f"DEBUG: Final output - {len(columns)} columns, {len(formatted_data)} rows")
        if len(formatted_data) > 0:
            print(f"DEBUG: Sample record keys: {list(formatted_data[0].keys())}")

        return columns, formatted_data

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
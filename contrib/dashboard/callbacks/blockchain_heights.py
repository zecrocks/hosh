from dash.dependencies import Input, Output, State
from dash import callback_context, html, no_update
from data.redis_client import fetch_blockchain_heights, clear_explorer_data
from data.nats_client import publish_http_check_trigger, trigger_http_checks
import asyncio
from collections import Counter
import redis
import json

def register_callbacks(app):
    """
    Register blockchain heights callbacks.
    """
    @app.callback(
        [Output('heights-table', 'data'),
         Output('heights-table', 'columns')],
        [Input('clear-explorers-button', 'n_clicks'),
         Input('auto-refresh-interval', 'n_intervals'),
         Input('current-page', 'data')]
    )
    def update_heights_table(clear_explorers_clicks, auto_refresh_intervals, current_page):
        """
        Update the blockchain heights table based on Redis data.
        Sort columns based on the number of items in each column.
        """
        # Only update if we're on the explorer heights page
        if current_page != 'blockchain-heights':
            return [], [
                {'name': 'Source', 'id': 'Source'},
                {'name': 'Coin', 'id': 'Coin'},
                {'name': 'Height', 'id': 'Height', 'type': 'numeric', 'format': {'specifier': ','}}
            ]
        
        ctx = callback_context
        button_id = ctx.triggered[0]['prop_id'].split('.')[0] if ctx.triggered else None

        # Handle clear button
        if button_id == 'clear-explorers-button':
            clear_explorer_data()
            return [], [
                {'name': 'Source', 'id': 'Source'},
                {'name': 'Coin', 'id': 'Coin'},
                {'name': 'Height', 'id': 'Height', 'type': 'numeric', 'format': {'specifier': ','}}
            ]

        # Regular update
        heights_data = fetch_blockchain_heights()
        
        # If no data, return default columns
        if not heights_data:
            return [], [
                {'name': 'Source', 'id': 'Source'},
                {'name': 'Coin', 'id': 'Coin'},
                {'name': 'Height', 'id': 'Height', 'type': 'numeric', 'format': {'specifier': ','}}
            ]
        
        # Count occurrences of each column value
        column_counts = {}
        for column in ['Source', 'Coin', 'Height']:
            # Count non-empty values in each column
            values = [row.get(column) for row in heights_data if row.get(column)]
            column_counts[column] = len(values)
        
        # Sort columns by count (descending)
        sorted_columns = sorted(column_counts.keys(), key=lambda col: column_counts[col], reverse=True)
        
        # Create columns list in the sorted order
        columns = []
        for col in sorted_columns:
            if col == 'Height':
                columns.append({'name': col, 'id': col, 'type': 'numeric', 'format': {'specifier': ','}})
            else:
                columns.append({'name': col, 'id': col})
        
        return heights_data, columns 

    @app.callback(
        Output('blockchain-heights', 'children'),
        Input('interval-component', 'n_intervals')
    )
    def update_blockchain_heights(n):
        redis_client = redis.Redis(host='redis', port=6379, db=0)
        
        # Get heights from different checkers
        btc_heights = get_btc_heights(redis_client)
        zec_heights = get_zec_heights(redis_client)
        http_heights = get_http_heights(redis_client)  # Add HTTP heights
        
        # Combine all heights
        all_heights = {
            **btc_heights,
            **zec_heights,
            **http_heights
        }
        
        # Create table rows
        rows = []
        for source, data in sorted(all_heights.items()):
            status_class = 'text-success' if data['status'] == 'online' else 'text-danger'
            last_updated = data.get('last_updated', 'N/A')  # Handle missing timestamp
            
            rows.append(html.Tr([
                html.Td(source),
                html.Td(data['height'], className=status_class),
                html.Td(last_updated),
                html.Td(data['status'], className=status_class)
            ]))
        
        return html.Table(
            [html.Thead(
                html.Tr([
                    html.Th("Source"),
                    html.Th("Height"),
                    html.Th("Last Updated"),
                    html.Th("Status")
                ])
            ),
            html.Tbody(rows)],
            className="table table-striped"
        )

    @app.callback(
        Output("http-trigger-result", "children"),
        Input("trigger-http-button", "n_clicks"),
        State("http-dry-run-toggle", "value"),
        prevent_initial_call=True
    )
    def handle_http_checks_trigger(n_clicks, dry_run):
        if n_clicks is None:
            return no_update
            
        try:
            from data.nats_client import trigger_http_checks
            success = trigger_http_checks(dry_run=dry_run)
            
            if success:
                return html.Div(f"✅ HTTP checks triggered successfully! {'(Dry Run)' if dry_run else ''}", className="text-success")
            else:
                return html.Div("❌ Failed to trigger checks", className="text-danger")
        except Exception as e:
            return html.Div(f"❌ Failed to trigger checks: {str(e)}", className="text-danger")

def get_http_heights(redis_client):
    """Get all HTTP explorer heights from Redis"""
    heights = {}
    # Scan for all http:* keys
    cursor = 0
    pattern = "http:*"
    
    while True:
        cursor, keys = redis_client.scan(cursor, match=pattern)
        for key in keys:
            try:
                # Key format is http:source.chain
                source_chain = key.decode('utf-8').replace('http:', '')
                height = redis_client.get(key)
                if height:
                    # Parse height (stored as string)
                    height = int(height.decode('utf-8'))
                    heights[source_chain] = {
                        'height': height,
                        'status': 'online',
                        'last_updated': None  # HTTP checker doesn't store timestamp in Redis
                    }
            except (ValueError, AttributeError) as e:
                print(f"Error parsing HTTP height for {key}: {e}")
        
        if cursor == 0:
            break
            
    return heights 
from dash.dependencies import Input, Output
from dash import callback_context
from data.redis_client import fetch_blockchain_heights, clear_explorer_data
from data.nats_client import publish_http_check_trigger
import asyncio
from collections import Counter

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
        Output("http-trigger-result", "children"),
        Input("trigger-http-button", "n_clicks"),
        prevent_initial_call=True
    )
    def trigger_http_checks(n_clicks):
        if n_clicks:
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            success = loop.run_until_complete(publish_http_check_trigger())
            loop.close()
            
            if success:
                return html.Div("✅ HTTP checks triggered successfully!", className="text-success")
            else:
                return html.Div("❌ Failed to trigger HTTP checks. See console for details.", className="text-danger")
        return "" 
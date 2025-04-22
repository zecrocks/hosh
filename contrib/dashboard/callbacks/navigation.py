from dash.dependencies import Input, Output
from dash import callback_context
from layouts.server_status import create_layout as create_server_status_layout
from layouts.blockchain_heights import create_layout as create_blockchain_heights_layout
from layouts.clickhouse_data import create_layout as create_clickhouse_data_layout

def register_callbacks(app):
    """
    Register navigation callbacks.
    """
    @app.callback(
        Output('auto-refresh-interval', 'interval'),
        Input('refresh-interval-input', 'value')
    )
    def update_interval(refresh_interval):
        return max(1, refresh_interval or 10) * 1000

    @app.callback(
        [Output('page-content', 'children'),
         Output('current-page', 'data')],
        [Input('server-status-link', 'n_clicks'),
         Input('blockchain-heights-link', 'n_clicks'),
         Input('clickhouse-data-link', 'n_clicks')],
        prevent_initial_call=True
    )
    def display_page(server_status_clicks, blockchain_heights_clicks, clickhouse_data_clicks):
        """
        Handle navigation between pages.
        """
        ctx = callback_context
        if not ctx.triggered:
            return create_server_status_layout(), 'server-status'
            
        button_id = ctx.triggered[0]['prop_id'].split('.')[0]
        
        if button_id == 'server-status-link':
            return create_server_status_layout(), 'server-status'
        elif button_id == 'blockchain-heights-link':
            return create_blockchain_heights_layout(), 'blockchain-heights'
        elif button_id == 'clickhouse-data-link':
            return create_clickhouse_data_layout(), 'clickhouse-data'
        
        # Default to server status
        return create_server_status_layout(), 'server-status' 
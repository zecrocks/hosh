from dash.dependencies import Input, Output
from dash import callback_context
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
        [Input('clickhouse-data-link', 'n_clicks')],
        prevent_initial_call=True
    )
    def display_page(clickhouse_data_clicks):
        """
        Handle navigation between pages.
        """
        ctx = callback_context
        if not ctx.triggered:
            return create_clickhouse_data_layout(), 'clickhouse-data'
            
        button_id = ctx.triggered[0]['prop_id'].split('.')[0]
        
        if button_id == 'clickhouse-data-link':
            return create_clickhouse_data_layout(), 'clickhouse-data'
        
        # Default to clickhouse data (only page now)
        return create_clickhouse_data_layout(), 'clickhouse-data' 
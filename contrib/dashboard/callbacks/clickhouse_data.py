from dash.dependencies import Input, Output, State
import plotly.express as px
import plotly.graph_objects as go
import pandas as pd
from data.clickhouse_client import fetch_server_stats, fetch_server_performance, get_server_list

def register_callbacks(app):
    """
    Register Clickhouse data callbacks.
    """
    @app.callback(
        [Output('server-stats-table', 'data'),
         Output('server-selector', 'options')],
        [Input('auto-refresh-interval', 'n_intervals'),
         Input('time-range-selector', 'value'),
         Input('current-page', 'data')]
    )
    def update_clickhouse_data(auto_refresh_intervals, time_range, current_page):
        """
        Update the Clickhouse data tables and dropdowns.
        """
        # Only update if we're on the Clickhouse data page
        if current_page != 'clickhouse-data':
            return [], []
        
        # Fetch server stats
        stats = fetch_server_stats(time_range)
        
        # Get server list for dropdown
        servers = get_server_list()
        
        return stats, servers

    @app.callback(
        [Output('response-time-graph', 'figure'),
         Output('success-rate-graph', 'figure')],
        [Input('server-selector', 'value'),
         Input('time-range-selector', 'value'),
         Input('auto-refresh-interval', 'n_intervals')],
        [State('current-page', 'data')]
    )
    def update_performance_graphs(server_value, time_range, auto_refresh_intervals, current_page):
        """
        Update the performance graphs based on selected server.
        """
        # Only update if we're on the Clickhouse data page
        if current_page != 'clickhouse-data' or not server_value:
            # Return empty figures
            empty_fig = go.Figure()
            empty_fig.update_layout(
                title="No server selected",
                xaxis=dict(title="Time"),
                yaxis=dict(title="Value"),
                template="plotly_white",
                height=400,
                margin=dict(l=50, r=50, t=50, b=50)
            )
            return empty_fig, empty_fig
        
        # Parse server value
        try:
            host, port, protocol = server_value.split(':')
            port = int(port)
        except Exception as e:
            print(f"Error parsing server value: {e}")
            empty_fig = go.Figure()
            empty_fig.update_layout(
                title="Error parsing server data",
                template="plotly_white",
                height=400
            )
            return empty_fig, empty_fig
        
        # Fetch performance data
        df = fetch_server_performance(host, port, protocol, time_range)
        
        if df.empty:
            empty_fig = go.Figure()
            empty_fig.update_layout(
                title="No data available for selected server",
                template="plotly_white",
                height=400
            )
            return empty_fig, empty_fig
        
        # Create response time graph
        response_fig = px.line(
            df, 
            x='time_interval', 
            y='avg_response_time',
            title=f"Average Response Time - {host}:{port} ({protocol})",
            labels={'time_interval': 'Time', 'avg_response_time': 'Response Time (ms)'}
        )
        response_fig.update_layout(
            template="plotly_white",
            height=400,
            margin=dict(l=50, r=50, t=50, b=50)
        )
        
        # Create success rate graph
        success_fig = px.line(
            df, 
            x='time_interval', 
            y='success_rate',
            title=f"Success Rate - {host}:{port} ({protocol})",
            labels={'time_interval': 'Time', 'success_rate': 'Success Rate'}
        )
        success_fig.update_layout(
            template="plotly_white",
            height=400,
            margin=dict(l=50, r=50, t=50, b=50),
            yaxis=dict(tickformat='.0%', range=[0, 1])
        )
        
        return response_fig, success_fig 
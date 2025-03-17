from dash.dependencies import Input, Output, State
import plotly.express as px
import plotly.graph_objects as go
import pandas as pd
from data.clickhouse_client import fetch_server_stats, fetch_server_performance, get_server_list, fetch_targets

def register_callbacks(app):
    """
    Register Clickhouse data callbacks.
    """
    @app.callback(
        [Output('server-stats-table', 'data'),
         Output('server-selector', 'options'),
         Output('targets-table', 'data'),
         Output('server-selector', 'value')],
        [Input('refresh-button', 'n_clicks'),
         Input('time-range-selector', 'value'),
         Input('current-page', 'data'),
         Input('targets-table', 'selected_rows')],
        [State('targets-table', 'data')]
    )
    def update_clickhouse_data(n_clicks, time_range, current_page, selected_rows, targets_data):
        """
        Update the Clickhouse data tables and dropdowns.
        """
        # Only update if we're on the Clickhouse data page
        if current_page != 'clickhouse-data':
            return [], [], [], None
        
        # Fetch targets data first
        targets = fetch_targets(time_range)
        
        # Get server list for dropdown
        servers = get_server_list()
        
        # Fetch server stats with the selected time range
        stats = fetch_server_stats(time_range)
        
        # Initialize selected_value as None
        selected_value = None
        
        # Filter stats if a target row is selected and update dropdown selection
        if selected_rows and targets_data:
            selected_target = targets_data[selected_rows[0]]
            stats = [
                stat for stat in stats 
                if stat['host'] == selected_target['hostname'] 
                and stat['protocol'] == selected_target['module']
            ]
            # Set the dropdown value to match the selected target
            selected_value = f"{selected_target['hostname']}::{selected_target['module']}"
        
        return stats, servers, targets, selected_value
    
    @app.callback(
        Output('server-performance-graph', 'figure'),
        [Input('server-selector', 'value'),
         Input('time-range-selector', 'value'),
         Input('refresh-button', 'n_clicks')],
        [State('current-page', 'data')],
        prevent_initial_call=False
    )
    def update_performance_graph(selected_server, time_range, n_clicks, current_page):
        """
        Update the server performance graph based on selected server and time range.
        """
        # Only update if we're on the Clickhouse data page
        if current_page != 'clickhouse-data' or not selected_server:
            return {
                'data': [],
                'layout': {
                    'title': 'Select a server to view performance data',
                    'xaxis': {'title': 'Time'},
                    'yaxis': {'title': 'Response Time (ms)'}
                }
            }
        
        # Parse the server value (format: "host:port:protocol")
        try:
            host, port, protocol = selected_server.split(':')
        except ValueError:
            return {
                'data': [],
                'layout': {
                    'title': 'Invalid server selection',
                    'xaxis': {'title': 'Time'},
                    'yaxis': {'title': 'Response Time (ms)'}
                }
            }
        
        # Fetch performance data for the selected server and time range
        performance_data = fetch_server_performance(host, protocol, time_range)
        
        if not performance_data:
            return {
                'data': [],
                'layout': {
                    'title': f'No data available for {host} ({protocol})',
                    'xaxis': {'title': 'Time'},
                    'yaxis': {'title': 'Response Time (ms)'}
                }
            }
        
        # Create a DataFrame from the performance data
        df = pd.DataFrame(performance_data)
        
        # Create the performance graph
        fig = px.scatter(
            df, 
            x='checked_at', 
            y='ping_ms',
            color='status',
            color_discrete_map={'online': 'green', 'offline': 'red'},
            title=f'Performance for {host} ({protocol})',
            labels={'checked_at': 'Time', 'ping_ms': 'Response Time (ms)', 'status': 'Status'}
        )
        
        # Add a line for the average response time
        if 'online' in df['status'].values:
            avg_ping = df[df['status'] == 'online']['ping_ms'].mean()
            fig.add_hline(
                y=avg_ping,
                line_dash="dash",
                line_color="blue",
                annotation_text=f"Avg: {avg_ping:.2f} ms",
                annotation_position="top right"
            )
        
        # Update layout
        fig.update_layout(
            xaxis_title='Time',
            yaxis_title='Response Time (ms)',
            legend_title='Status',
            hovermode='closest'
        )
        
        return fig 
from dash.dependencies import Input, Output, State
import dash
import plotly.express as px
import pandas as pd
from data.clickhouse_client import fetch_server_stats, fetch_server_performance, get_server_list, fetch_targets, fetch_check_results
import logging
from datetime import datetime, timezone

logger = logging.getLogger(__name__)

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
         Input('targets-table', 'selected_rows'),
         Input('module-filter', 'value'),
         Input('use-manual-input-button', 'n_clicks')],
        [State('targets-table', 'data'),
         State('manual-server-input', 'value'),
         State('protocol-selector', 'value')]
    )
    def update_clickhouse_data(n_clicks, time_range, current_page, selected_rows, module_filter, 
                               manual_input_clicks, targets_data, manual_server, protocol):
        """
        Update the Clickhouse data tables and dropdowns.
        """
        # Get the callback context to determine which input triggered the callback
        ctx = dash.callback_context
        triggered_id = ctx.triggered[0]['prop_id'].split('.')[0] if ctx.triggered else None
        
        # Only update if we're on the Clickhouse data page
        if current_page != 'clickhouse-data':
            return [], [], [], None
        
        # If triggered by "Use Manual Input" button, prioritize that
        if triggered_id == 'use-manual-input-button' and manual_server and protocol:
            # Format the manual input value
            formatted_value = f"{manual_server}::{protocol}"
            
            # We still need to fetch data for other outputs
            targets = fetch_targets(time_range)
            if module_filter != 'all':
                targets = [t for t in targets if t['module'] == module_filter]
            
            servers = get_server_list()
            stats = fetch_server_stats(time_range)
            
            return stats, servers, targets, formatted_value
        
        # Otherwise, handle normal table/dropdown updates
        targets = fetch_targets(time_range)
        
        # Filter targets based on module selection
        if module_filter != 'all':
            targets = [t for t in targets if t['module'] == module_filter]
        
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
        [Output('server-performance-graph', 'figure'),
         Output('check-results-table', 'data')],
        [Input('server-selector', 'value'),
         Input('time-range-selector', 'value'),
         Input('refresh-button', 'n_clicks')],
        [State('current-page', 'data')],
        prevent_initial_call=False
    )
    def update_performance_data(selected_server, time_range, n_clicks, current_page):
        """
        Update the server performance graph and results table based on selected server and time range.
        """
        logger.info(f"Callback triggered with: server={selected_server}, time_range={time_range}, page={current_page}")
        
        # Only update if we're on the Clickhouse data page
        if current_page != 'clickhouse-data' or not selected_server:
            logger.debug("Skipping update - not on clickhouse page or no server selected")
            return {
                'data': [],
                'layout': {
                    'title': 'Select a server to view performance data',
                    'xaxis': {'title': 'Time'},
                    'yaxis': {'title': 'Response Time (ms)'}
                }
            }, []
        
        try:
            host, protocol = selected_server.split('::')
            logger.info(f"Fetching data for host={host}, protocol={protocol}")
            
            # Fetch check results for the table
            check_results = fetch_check_results(host, protocol, time_range)
            logger.info(f"Fetched {len(check_results)} check results")
            
            # Fetch performance data for the graph
            performance_data = fetch_server_performance(host, protocol, time_range)
            logger.info(f"Fetched {len(performance_data)} performance records")
            
            if not performance_data:
                logger.info("No performance data available")
                return {
                    'data': [],
                    'layout': {
                        'title': f'No data available for {host} ({protocol})',
                        'xaxis': {'title': 'Time'},
                        'yaxis': {'title': 'Response Time (ms)'}
                    }
                }, check_results
            
            # Create the performance graph
            df = pd.DataFrame(performance_data)
            
            fig = px.scatter(
                df, 
                x='checked_at', 
                y='ping_ms',
                color='status',
                color_discrete_map={'online': 'green', 'offline': 'red'},
                title=f'Performance for {host} ({protocol})',
                labels={'checked_at': 'Time', 'ping_ms': 'Response Time (ms)', 'status': 'Status'}
            )
            
            # Add average response time line for online servers
            if 'online' in df['status'].values:
                avg_ping = df[df['status'] == 'online']['ping_ms'].mean()
                fig.add_hline(
                    y=avg_ping,
                    line_dash="dash",
                    line_color="blue",
                    annotation_text=f"Avg: {avg_ping:.2f} ms",
                    annotation_position="top right"
                )
            
            # Add vertical line for current time
            current_time = datetime.now(timezone.utc)
            fig.add_shape(
                type="line",
                x0=current_time,
                x1=current_time,
                y0=0,
                y1=1,
                yref="paper",
                line=dict(color="gray", width=2, dash="solid"),
            )
            
            # Add annotation for the current time line
            fig.add_annotation(
                x=current_time,
                y=1,
                yref="paper",
                text="Current Time",
                showarrow=False,
                textangle=-90,
                yanchor="bottom",
                font=dict(color="gray")
            )
            
            fig.update_layout(
                xaxis_title='Time',
                yaxis_title='Response Time (ms)',
                legend_title='Status',
                hovermode='closest'
            )
            
            return fig, check_results
            
        except Exception as e:
            logger.error(f"Error in callback: {e}", exc_info=True)
            return {
                'data': [],
                'layout': {
                    'title': 'Error fetching data',
                    'xaxis': {'title': 'Time'},
                    'yaxis': {'title': 'Response Time (ms)'}
                }
            }, []

    # Add callback to reset the manual input fields
    @app.callback(
        [Output('manual-server-input', 'value'),
         Output('protocol-selector', 'value')],
        Input('reset-manual-input-button', 'n_clicks'),
        prevent_initial_call=True
    )
    def reset_manual_input(n_clicks):
        """Reset the manual input fields to their default/empty values"""
        if not n_clicks:
            return dash.no_update, dash.no_update
        return "", "btc"


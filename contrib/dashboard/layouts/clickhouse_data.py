from dash import html, dcc, dash_table
import dash_bootstrap_components as dbc

def create_layout():
    """
    Create the Clickhouse data page layout.
    """
    return html.Div([
        html.H2("Server Performance Data", className='mb-3'),
        
        # Time range selector and refresh button
        html.Div([
            html.Div([
                html.Label("Time Range:", className='me-2'),
                dcc.RadioItems(
                    id='time-range-selector',
                    options=[
                        {'label': 'Last 10 Minutes', 'value': '10m'},
                        {'label': 'Last Hour', 'value': '1h'},
                        {'label': 'Last 24 Hours', 'value': '24h'},
                        {'label': 'Last 7 Days', 'value': '7d'},
                        {'label': 'Last 30 Days', 'value': '30d'},
                    ],
                    value='24h',
                    inline=True,
                ),
            ], className='me-3'),
            html.Button(
                'Refresh Data',
                id='refresh-button',
                className='btn btn-primary',
            ),
        ], className='d-flex align-items-center mb-3'),

        # Targets table
        html.Div([
            html.H3("Active Targets", className='mb-2'),
            dash_table.DataTable(
                id='targets-table',
                columns=[
                    {'name': 'Hostname', 'id': 'hostname'},
                    {'name': 'Module', 'id': 'module'},
                    {'name': 'Last Queued', 'id': 'last_queued_at'},
                    {'name': 'Last Checked', 'id': 'last_checked_at'},
                    {'name': 'User Submitted', 'id': 'user_submitted'},
                ],
                data=[],
                style_table={'overflowX': 'auto'},
                style_cell={'textAlign': 'left', 'padding': '5px'},
                style_header={'fontWeight': 'bold', 'backgroundColor': '#f4f4f4'},
                sort_action='native',
                page_size=10,
                row_selectable='single',
                selected_rows=[],
            ),
            html.Div(id='targets-loading-message', children="Loading targets...", 
                     style={'display': 'none'}, className='mt-2 text-muted'),
        ], className='mb-4'),

        # Server stats table
        html.Div([
            html.H3("Server Statistics", className='mb-2'),
            dash_table.DataTable(
                id='server-stats-table',
                columns=[
                    {'name': 'Host', 'id': 'host'},
                    {'name': 'Port', 'id': 'port'},
                    {'name': 'Protocol', 'id': 'protocol'},
                    {'name': 'Success Rate', 'id': 'success_rate'},
                    {'name': 'Avg Response Time', 'id': 'avg_response_time'},
                    {'name': 'Total Checks', 'id': 'total_checks'},
                    {'name': 'Last Check', 'id': 'last_check'},
                ],
                data=[],
                style_table={'overflowX': 'auto'},
                style_cell={'textAlign': 'left', 'padding': '5px'},
                style_header={'fontWeight': 'bold', 'backgroundColor': '#f4f4f4'},
                sort_action='native',
                page_size=10,
            ),
            html.Div(id='stats-loading-message', children="Loading server statistics...", 
                     style={'display': 'none'}, className='mt-2 text-muted'),
        ], className='mb-4'),
        
        
        # Server selector for detailed view
        html.Div([
            html.H3("Server Performance Details", className='mb-2'),
            html.Div([
                html.Label("Select Server:", className='me-2'),
                dcc.Dropdown(
                    id='server-selector',
                    options=[],
                    placeholder="Select a server to view details",
                    className='mb-3'
                ),
            ]),
            
            # Performance graph
            dcc.Graph(
                id='server-performance-graph',
                figure={'data': [], 'layout': {'title': 'Select a server to view performance data'}}
            ),
        ]),
    ]) 
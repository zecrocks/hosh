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
            # Add radio buttons for module filtering
            dbc.RadioItems(
                id='module-filter',
                options=[
                    {'label': 'Bitcoin', 'value': 'btc'},
                    {'label': 'Zcash', 'value': 'zec'},
                    {'label': 'HTTP', 'value': 'http'},
                    {'label': 'All', 'value': 'all'}
                ],
                value='all',  # Default to showing all modules
                inline=True,
                className="mb-3"
            ),
            dash_table.DataTable(
                id='targets-table',
                columns=[
                    {'name': 'Target ID', 'id': 'target_id'},
                    {'name': 'Hostname', 'id': 'hostname'},
                    {'name': 'Module', 'id': 'module'},
                    {'name': 'Port', 'id': 'port'},
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

        # Check results table
        html.Div([
            html.H3("Check Results", className='mb-2'),
            dash_table.DataTable(
                id='check-results-table',
                columns=[
                    {'name': 'Target ID', 'id': 'Target ID'},
                    {'name': 'Checked At', 'id': 'Checked At'},
                    {'name': 'Hostname', 'id': 'Hostname'},
                    {'name': 'IP Address', 'id': 'IP Address'},
                    {'name': 'IP Version', 'id': 'IP Version'},
                    {'name': 'Checker Module', 'id': 'Checker Module'},
                    {'name': 'Status', 'id': 'Status'},
                    {'name': 'Response Time (ms)', 'id': 'Response Time (ms)'},
                    {'name': 'Checker Location', 'id': 'Checker Location'},
                    {'name': 'Checker ID', 'id': 'Checker ID'},
                    {'name': 'Response Data', 'id': 'Response Data'},
                    {'name': 'User Submitted', 'id': 'User Submitted'}
                ],
                style_table={
                    'overflowX': 'auto',
                    'minWidth': '100%'
                },
                style_cell={
                    'textAlign': 'left',
                    'minWidth': '100px',
                    'maxWidth': '500px',
                    'overflow': 'hidden',
                    'textOverflow': 'ellipsis',
                    'padding': '5px'
                },
                style_header={
                    'backgroundColor': 'rgb(230, 230, 230)',
                    'fontWeight': 'bold'
                },
                style_data_conditional=[
                    {
                        'if': {'column_id': 'Status', 'filter_query': '{Status} eq "online"'},
                        'backgroundColor': '#dff0d8',
                        'color': '#3c763d'
                    },
                    {
                        'if': {'column_id': 'Status', 'filter_query': '{Status} eq "offline"'},
                        'backgroundColor': '#f2dede',
                        'color': '#a94442'
                    }
                ],
                tooltip_data=[],
                tooltip_duration=None,
                page_size=10,
                sort_action='native',
                sort_mode='multi',
                filter_action='native',
                row_selectable=False,
                selected_rows=[],
                page_current=0
            ),
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
                # Replace dropdown with a combo of dropdown and text input
                html.Div([
                    dcc.Dropdown(
                        id='server-selector',
                        options=[],
                        placeholder="Select a server to view details",
                        className='mb-3'
                    ),
                    html.Div([
                        html.Label("Or Enter Server Manually:", className='me-2 mt-2'),
                        html.Div([
                            dcc.Input(
                                id='manual-server-input',
                                type='text',
                                placeholder='Enter hostname (e.g., example.com)',
                                className='form-control mb-2',
                                style={'width': '100%'}
                            ),
                            dcc.Dropdown(
                                id='protocol-selector',
                                options=[
                                    {'label': 'Bitcoin', 'value': 'btc'},
                                    {'label': 'Zcash', 'value': 'zec'},
                                    {'label': 'HTTP', 'value': 'http'}
                                ],
                                placeholder="Select protocol",
                                value='btc',
                                className='mb-3'
                            )
                        ], className='d-flex flex-column'),
                        html.Button(
                            'Use Manual Input',
                            id='use-manual-input-button',
                            className='btn btn-primary mb-3 mt-2',
                        ),
                        html.Button(
                            'Reset',
                            id='reset-manual-input-button',
                            className='btn btn-secondary mb-3 mt-2 ms-2',
                        )
                    ], className='mb-3')
                ]),
            ]),
            
            # Performance graph
            dcc.Graph(
                id='server-performance-graph',
                figure={'data': [], 'layout': {'title': 'Select a server to view performance data'}}
            ),
        ]),
    ]) 
from dash import html, dcc, dash_table
import dash_bootstrap_components as dbc

def create_layout():
    """
    Create the Clickhouse data page layout.
    """
    return html.Div([
        html.H2("Server Performance Data", className='mb-3'),
        
        # Time range selector
        html.Div([
            html.Label("Time Range:", className='me-2'),
            dcc.RadioItems(
                id='time-range-selector',
                options=[
                    {'label': 'Last 24 Hours', 'value': '24h'},
                    {'label': 'Last 7 Days', 'value': '7d'},
                    {'label': 'Last 30 Days', 'value': '30d'},
                ],
                value='24h',
                inline=True,
                className='mb-3'
            ),
        ]),
        
        # Server stats table
        html.Div([
            html.H3("Server Statistics (24h)", className='mb-2'),
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
        
        # Server selector for graphs
        html.Div([
            html.H3("Performance Graphs", className='mb-2'),
            html.Div([
                html.Label("Select Server:", className='me-2'),
                dcc.Dropdown(
                    id='server-selector',
                    options=[],
                    value=None,
                    placeholder="Select a server to view performance data",
                    className='mb-3'
                ),
            ]),
        ]),
        
        # Graphs
        dbc.Row([
            dbc.Col([
                dcc.Graph(id='response-time-graph')
            ], width=12, lg=6),
            dbc.Col([
                dcc.Graph(id='success-rate-graph')
            ], width=12, lg=6),
        ]),
        
        html.Div(id='graph-loading-message', children="Select a server to view performance graphs", 
                 className='mt-2 text-muted text-center'),
    ]) 
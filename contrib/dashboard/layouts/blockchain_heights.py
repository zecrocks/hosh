from dash import html, dcc, dash_table
import dash_bootstrap_components as dbc
from data.clickhouse_client import get_client

def get_http_targets():
    """Get HTTP targets from Clickhouse."""
    try:
        query = """
        SELECT DISTINCT hostname
        FROM targets
        WHERE module = 'http'
        ORDER BY hostname
        """
        
        with get_client() as client:
            result = client.execute(query)
            
        return [{'label': hostname, 'value': hostname} for (hostname,) in result]
        
    except Exception as e:
        print(f"Error fetching HTTP targets from ClickHouse: {e}")
        return []

def create_layout():
    """Create the blockchain heights page layout."""
    http_targets = get_http_targets()
    
    return html.Div([
        html.H1("Blockchain Heights", className="mb-4"),
        
        # HTTP Checks Section
        html.Div([
            html.H2("HTTP Block Explorer Checks", className="mb-3"),
            dbc.Row([
                dbc.Col([
                    dbc.Label("Select Block Explorer URL:"),
                    dcc.Dropdown(
                        id='http-target-dropdown',
                        options=http_targets,
                        value=http_targets[0]['value'] if http_targets else None,
                        className="mb-3"
                    ),
                    dbc.Switch(
                        id='dry-run-switch',
                        label="Dry Run",
                        value=False,
                        className="mb-3"
                    ),
                    dbc.Button(
                        "Trigger HTTP Check",
                        id="trigger-http-check-button",
                        color="primary",
                        className="mb-3"
                    ),
                    # Add div for trigger result message
                    html.Div(id="http-trigger-result", className="mb-3")
                ], width=6)
            ]),
            
            # Results Table
            html.Div([
                html.H3("Results", className="mb-3"),
                dash_table.DataTable(
                    id='explorer-heights-table',
                    columns=[
                        {'name': 'Hostname', 'id': 'hostname'},
                        {'name': 'Module', 'id': 'module'},
                        {'name': 'Last Check', 'id': 'last_check'},
                        {'name': 'Last Height', 'id': 'last_height'},
                        {'name': 'Response Time', 'id': 'response_time'}
                    ],
                    data=[],
                    style_table={'overflowX': 'auto'},
                    style_cell={
                        'textAlign': 'left',
                        'padding': '10px',
                        'whiteSpace': 'normal'
                    },
                    style_header={
                        'backgroundColor': 'rgb(230, 230, 230)',
                        'fontWeight': 'bold'
                    },
                    style_data_conditional=[
                        {
                            'if': {'row_index': 'odd'},
                            'backgroundColor': 'rgb(248, 248, 248)'
                        }
                    ]
                )
            ])
        ])
    ]) 
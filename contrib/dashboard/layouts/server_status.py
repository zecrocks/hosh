from dash import html, dash_table
import dash_bootstrap_components as dbc
from data.clickhouse_client import clickhouse_client

def get_server_count_by_type():
    """Get count of servers by type from Clickhouse."""
    if not clickhouse_client:
        return {"btc": 0, "zec": 0}
        
    try:
        query = """
            SELECT module, count(DISTINCT hostname) as count
            FROM targets
            WHERE module IN ('btc', 'zec')
            GROUP BY module
        """
        results = clickhouse_client.execute(query)
        
        counts = {"btc": 0, "zec": 0}
        for row in results:
            module, count = row
            counts[module] = count
            
        return counts
    except Exception as e:
        print(f"Error getting server counts from Clickhouse: {e}")
        return {"btc": 0, "zec": 0}

def create_layout():
    """
    Create the light nodes page layout.
    """
    # Get server counts for the button labels
    server_counts = get_server_count_by_type()
    
    return html.Div([
        html.Div([
            # Add radio buttons for network filtering
            dbc.RadioItems(
                id='network-filter',
                options=[
                    {'label': f'Bitcoin ({server_counts["btc"]})', 'value': 'btc'},
                    {'label': f'Zcash ({server_counts["zec"]})', 'value': 'zec'},
                    {'label': 'All', 'value': 'all'}
                ],
                value='all',  # Default to showing all networks
                inline=True,
                className="mb-3"
            ),
            
            # Keep only BTC and ZEC check buttons
            dbc.Button(
                f"Trigger BTC Checks ({server_counts['btc']})", 
                id="trigger-btc-button", 
                color="primary",
                className="me-2"
            ),
            dbc.Button(
                f"Trigger ZEC Checks ({server_counts['zec']})", 
                id="trigger-zec-button", 
                color="primary"
            ),
        ], className="mb-3 d-flex flex-wrap gap-2"),
        
        # Update results area to remove HTTP trigger result
        html.Div([
            html.Div(id="btc-trigger-result", className="me-2"),
            html.Div(id="zec-trigger-result")
        ], className="mb-3 d-flex flex-wrap gap-2"),
        
        html.Div([
            html.H2("Light Nodes", className='mb-3'),
            dash_table.DataTable(
                id='servers-table',
                columns=[
                    {'name': 'Server', 'id': 'hostname', 'type': 'text'},
                    {'name': 'Chain', 'id': 'chain', 'type': 'text'},
                    {'name': 'Status', 'id': 'status', 'type': 'text'},
                    {'name': 'Block Height', 'id': 'block_height', 'type': 'numeric', 'format': {'specifier': ','}},
                    {'name': 'Response Time (ms)', 'id': 'response_time_ms', 'type': 'numeric', 'format': {'specifier': '.1f'}},
                    {'name': 'Last Checked', 'id': 'checked_at', 'type': 'text'},
                    {'name': 'Error', 'id': 'error', 'type': 'text'}
                ],
                data=[],
                style_table={'overflowX': 'auto'},
                style_cell={'textAlign': 'left', 'padding': '5px'},
                style_header={'fontWeight': 'bold', 'backgroundColor': '#f4f4f4'},
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
                sort_action='native',
                filter_action='native',
                page_action='native',
                page_size=20,
            ),
        ]),
    ]) 
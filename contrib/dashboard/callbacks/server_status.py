from dash import Input, Output, State, callback_context
from data.nats_client import publish_chain_check_trigger
from data.clickhouse_client import get_client
from datetime import datetime, timezone
import logging
import asyncio

logger = logging.getLogger(__name__)

def register_callbacks(app, long_callback_manager):
    """
    Register server status callbacks.
    
    Args:
        app: Dash application instance
        long_callback_manager: Long callback manager instance (unused but required by interface)
    """
    @app.callback(
        [Output("servers-table", "data"),
         Output("btc-trigger-result", "children"),
         Output("zec-trigger-result", "children")],
        [Input("trigger-btc-button", "n_clicks"),
         Input("trigger-zec-button", "n_clicks"),
         Input("auto-refresh-interval", "n_intervals"),
         Input("network-filter", "value")],
        []
    )
    def update_server_status(btc_clicks, zec_clicks, n_intervals, network_filter):
        """
        Update the server status table with data from Clickhouse.
        """
        ctx = callback_context
        trigger_id = ctx.triggered[0]["prop_id"].split(".")[0] if ctx.triggered else None
        
        # Handle button clicks
        if trigger_id == "trigger-btc-button":
            try:
                loop = asyncio.new_event_loop()
                asyncio.set_event_loop(loop)
                success = loop.run_until_complete(publish_chain_check_trigger('btc'))
                loop.close()
                
                if success:
                    btc_result = "✅ BTC checks triggered successfully"
                else:
                    btc_result = "❌ Failed to trigger BTC checks"
            except Exception as e:
                logger.error(f"Error triggering BTC checks: {e}")
                btc_result = f"❌ Error triggering BTC checks: {str(e)}"
        else:
            btc_result = ""
            
        if trigger_id == "trigger-zec-button":
            try:
                loop = asyncio.new_event_loop()
                asyncio.set_event_loop(loop)
                success = loop.run_until_complete(publish_chain_check_trigger('zec'))
                loop.close()
                
                if success:
                    zec_result = "✅ ZEC checks triggered successfully"
                else:
                    zec_result = "❌ Failed to trigger ZEC checks"
            except Exception as e:
                logger.error(f"Error triggering ZEC checks: {e}")
                zec_result = f"❌ Error triggering ZEC checks: {str(e)}"
        else:
            zec_result = ""
        
        # Fetch data from Clickhouse
        try:
            query = """
                SELECT 
                    hostname,
                    checker_module as chain,
                    status,
                    JSONExtractString(response_data, 'height') as block_height,
                    ping_ms as response_time_ms,
                    checked_at,
                    JSONExtractString(response_data, 'error') as error
                FROM results
                WHERE checker_module IN ('btc', 'zec')
                AND checked_at >= now() - INTERVAL 1 HOUR
            """
            
            if network_filter != 'all':
                query += f" AND checker_module = '{network_filter}'"
                
            query += " ORDER BY checked_at DESC"
            
            with get_client() as client:
                results = client.execute(query, with_column_types=True)
            
            rows, columns = results
            
            # Convert results to list of dictionaries
            data = []
            for row in rows:
                data.append({
                    'hostname': row[0],
                    'chain': row[1],
                    'status': row[2],
                    'block_height': int(row[3]) if row[3] else None,
                    'response_time_ms': float(row[4]) if row[4] else None,
                    'checked_at': row[5].astimezone(timezone.utc).strftime('%Y-%m-%d %H:%M:%S UTC'),
                    'error': row[6] if row[6] else ''
                })
                
            return data, btc_result, zec_result
            
        except Exception as e:
            logger.error(f"Error fetching server status data: {e}")
            return [], f"Error fetching data: {str(e)}", ""

def get_server_stats():
    """Get server statistics from Clickhouse."""
    try:
        query = """
        SELECT
            hostname,
            checker_module,
            count(*) as total_checks,
            countIf(status = 'online') as successful_checks,
            avg(if(status = 'online', ping_ms, null)) as avg_response_time,
            max(checked_at) as last_check
        FROM results
        WHERE checked_at >= now() - INTERVAL 24 HOUR
        GROUP BY hostname, checker_module
        ORDER BY hostname, checker_module
        """
        
        with get_client() as client:
            result = client.execute(query)
            
        stats = []
        for row in result:
            hostname, module, total, success, avg_time, last_check = row
            stats.append({
                'hostname': hostname,
                'module': module,
                'total_checks': total,
                'success_rate': f"{(success/total)*100:.1f}%" if total > 0 else "0%",
                'avg_response_time': f"{avg_time:.1f}ms" if avg_time else "N/A",
                'last_check': last_check.strftime('%Y-%m-%d %H:%M:%S') if last_check else 'Never'
            })
        return stats
        
    except Exception as e:
        print(f"Error fetching server stats from ClickHouse: {e}")
        return [] 
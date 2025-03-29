import os
import json
import asyncio
import nats
from .clickhouse_client import clickhouse_client
from datetime import datetime, timezone

# NATS Configuration
NATS_HOST = os.environ.get('NATS_HOST', 'nats')
NATS_PORT = int(os.environ.get('NATS_PORT', 4222))
NATS_URL = f"nats://{NATS_HOST}:{NATS_PORT}"
NATS_PREFIX = os.environ.get('NATS_PREFIX', 'hosh.')  # Match Rust config default


async def publish_http_check_trigger(url=None, dry_run=False):
    """
    Publish a message to trigger HTTP checks.
    
    Args:
        url (str, optional): The URL of the block explorer to check. If None, triggers all checks.
        dry_run (bool): If True, only simulate the checks without making actual requests
    """
    try:
        # Connect to NATS
        nc = await nats.connect(NATS_URL)
        
        # Prepare the message - exactly matching Rust format
        message = {
            "url": url or "",  # Use provided URL or empty string to trigger all checks
            "port": 80,
            "check_id": None,
            "user_submitted": False,
            "dry_run": dry_run
        }
        
        # Use same subject format as Rust code
        subject = f"{NATS_PREFIX}check.http"
        
        # Publish the message
        await nc.publish(subject, json.dumps(message).encode())
        print(f"Published HTTP check trigger to NATS subject: {subject} (url={url}, dry_run={dry_run})")
        
        # Close NATS connection
        await nc.close()
        return True
        
    except Exception as e:
        print(f"Error triggering HTTP checks: {e}")
        return False


def trigger_http_checks(url=None, dry_run=False):
    """
    Trigger HTTP checks via NATS.
    
    Args:
        url (str, optional): The URL of the block explorer to check. If None, triggers all checks.
        dry_run (bool): If True, only simulate the checks without making actual requests
    """
    try:
        # Run the async function
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        result = loop.run_until_complete(publish_http_check_trigger(url=url, dry_run=dry_run))
        loop.close()
        return result
    except Exception as e:
        print(f"Error in trigger_http_checks: {e}")
        return False


async def publish_chain_check_trigger(chain_type, specific_host=None):
    """
    Publish a message to trigger checks for a specific blockchain or server.
    
    Args:
        chain_type (str): The chain type, e.g., 'btc' or 'zec'
        specific_host (str, optional): If provided, only trigger check for this host
    """
    try:
        print(f"Starting chain check trigger for {chain_type}" + (f" (specific host: {specific_host})" if specific_host else ""))
        
        # Connect to NATS
        print("Connecting to NATS...")
        nc = await nats.connect(NATS_URL)
        print("Successfully connected to NATS")
        
        if not clickhouse_client:
            print(f"Clickhouse client not available")
            return False
        
        # Query to get unique hostnames for the chain type
        query = f"""
            SELECT DISTINCT hostname
            FROM targets
            WHERE module = '{chain_type}'
            AND last_checked_at < now() - INTERVAL 5 MINUTE
        """
        
        if specific_host:
            query += f" AND hostname = '{specific_host}'"
            
        print(f"Executing Clickhouse query: {query}")
        
        results = clickhouse_client.execute(query)
        print(f"Query returned {len(results) if results else 0} results")
        
        if not results:
            print(f"No {chain_type} servers found in Clickhouse")
            return False
            
        # Publish a check request for each host
        count = 0
        for (hostname,) in results:
            try:
                # Create message matching the CheckRequest struct
                message = {
                    "host": hostname,
                    "port": 50002 if chain_type == 'btc' else 9067,
                    "version": "unknown",
                    "check_id": None,
                    "user_submitted": False
                }
                
                subject = f"hosh.check.{chain_type}"
                await nc.publish(subject, json.dumps(message).encode())
                count += 1
                print(f"Published check request for {hostname}")
                
            except Exception as e:
                print(f"Error publishing check request for {hostname}: {e}")
                continue
                
        print(f"Successfully published {count} check requests")
        await nc.close()
        return True
        
    except Exception as e:
        print(f"Error triggering {chain_type} checks: {e}")
        return False 
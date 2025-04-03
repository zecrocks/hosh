import os
import json
import asyncio
import nats
import uuid
from .clickhouse_client import get_client
from datetime import datetime, timezone
import threading

# NATS Configuration
NATS_HOST = os.environ.get('NATS_HOST', 'nats')
NATS_PORT = int(os.environ.get('NATS_PORT', 4222))
NATS_URL = f"nats://{NATS_HOST}:{NATS_PORT}"
NATS_PREFIX = os.environ.get('NATS_PREFIX', 'hosh.')  # Match Rust config default

# Global NATS client
_nats_client = None
_nats_lock = threading.Lock()

async def get_nats_client():
    """Get or create NATS client."""
    global _nats_client
    with _nats_lock:
        if _nats_client is None or not _nats_client.is_connected:
            try:
                print(f"Attempting to connect to NATS at {NATS_URL}")
                _nats_client = await nats.connect(NATS_URL)
                print(f"Successfully connected to NATS at {NATS_URL}")
            except Exception as e:
                print(f"Failed to connect to NATS: {e}")
                import traceback
                print("Full traceback:", traceback.format_exc())
                return None
        else:
            print("Using existing NATS connection")
        return _nats_client

async def close_nats_client():
    """Close NATS client connection."""
    global _nats_client
    with _nats_lock:
        if _nats_client and _nats_client.is_connected:
            try:
                await _nats_client.drain()
                await _nats_client.close()
                _nats_client = None
                print("NATS connection closed")
            except Exception as e:
                print(f"Error closing NATS connection: {e}")

async def publish_http_check_trigger(url=None, dry_run=False):
    """
    Publish a message to trigger HTTP checks.
    
    Args:
        url (str, optional): The URL of the block explorer to check. If None, triggers all checks.
        dry_run (bool): If True, only simulate the checks without making actual requests
    """
    try:
        print(f"Starting publish_http_check_trigger with url={url}, dry_run={dry_run}")
        
        # Get NATS client
        print("Getting NATS client")
        nc = await get_nats_client()
        if not nc:
            print("Failed to get NATS client")
            return False
        
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
        
        print(f"Preparing to publish message to subject {subject}: {message}")
        
        # Publish the message
        await nc.publish(subject, json.dumps(message).encode())
        print(f"Successfully published HTTP check trigger to NATS subject: {subject}")
        return True
        
    except Exception as e:
        print(f"Error triggering HTTP checks: {e}")
        import traceback
        print("Full traceback:", traceback.format_exc())
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

async def publish_chain_check_trigger(chain_type, specific_host=None, user_submitted=False):
    """
    Publish a message to trigger checks for a specific blockchain or server.
    
    Args:
        chain_type (str): The chain type, e.g., 'btc' or 'zec'
        specific_host (str, optional): If provided, only trigger check for this host
        user_submitted (bool, optional): If True, mark as user-submitted and use user subject for BTC
    """
    try:
        print(f"Starting chain check trigger for {chain_type}" + 
              (f" (specific host: {specific_host})" if specific_host else "") +
              (f" (user-submitted: {user_submitted})" if user_submitted else ""))
        
        # Get NATS client
        nc = await get_nats_client()
        if not nc:
            return False
        
        # If we have a specific host, we don't need to query Clickhouse
        if specific_host:
            hosts = [(specific_host,)]
        else:
            # Query to get unique hostnames for the chain type
            query = f"""
                SELECT DISTINCT hostname
                FROM targets
                WHERE module = '{chain_type}'
                AND last_checked_at < now() - INTERVAL 5 MINUTE
            """
                
            print(f"Executing Clickhouse query: {query}")
            
            with get_client() as client:
                hosts = client.execute(query)
                print(f"Query returned {len(hosts) if hosts else 0} results")
            
            if not hosts:
                print(f"No {chain_type} servers found in Clickhouse")
                return False
            
        # Publish a check request for each host
        count = 0
        for (hostname,) in hosts:
            try:
                # Create message matching the CheckRequest struct
                message = {
                    "host": hostname,
                    "port": 50002 if chain_type == 'btc' else 9067,
                    "version": "unknown",
                    "check_id": str(uuid.uuid4()),  # Generate a UUID string for check_id
                    "user_submitted": user_submitted
                }
                
                # Use .user suffix for BTC user-submitted checks
                if chain_type == 'btc' and user_submitted:
                    subject = f"{NATS_PREFIX}check.{chain_type}.user"
                else:
                    subject = f"{NATS_PREFIX}check.{chain_type}"
                    
                print(f"Publishing to subject: {subject}")
                await nc.publish(subject, json.dumps(message).encode())
                count += 1
                print(f"Published check request for {hostname} to {subject}")
                
            except Exception as e:
                print(f"Error publishing check request for {hostname}: {e}")
                continue
                
        print(f"Successfully published {count} check requests")
        return True
        
    except Exception as e:
        print(f"Error triggering {chain_type} checks: {e}")
        return False 
#!/bin/sh
set -e  # Exit immediately if a command fails

# Clean up old daemon lockfiles if they exist
rm -f /root/.electrum/daemon /root/.electrum/daemon_rpc_socket

# Start the daemon
echo "Starting Electrum daemon..."
electrum daemon -d

# Wait until daemon is ready
echo "Waiting for daemon to initialize..."
sleep 5

# Fetch the Electrum servers list and dump to JSON
echo "Fetching Electrum servers..."
electrum getservers > /electrum/btc/servers.json

echo "Servers list saved to /electrum/btc/servers.json"

# Keep the container alive for debugging if needed
exec "$@"

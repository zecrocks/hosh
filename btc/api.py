from flask import Flask, request, jsonify
from flasgger import Swagger
import json
import subprocess
import socket
import ssl
import time
import re
import struct
import binascii
import datetime
import logging
import socks
import socket
import os

# Configure Tor SOCKS proxy from environment variables
TOR_PROXY_HOST = os.environ.get("TOR_PROXY_HOST", "tor")
TOR_PROXY_PORT = int(os.environ.get("TOR_PROXY_PORT", 9050))


# Configure logging
logging.basicConfig(
    level=logging.INFO,  # Set the logging level
    format='%(asctime)s - %(levelname)s - %(message)s'
)


app = Flask(__name__)

# Configure Swagger UI
app.config['SWAGGER'] = {
    'title': 'BTC worker',  # Set the tab title
    'uiversion': 3  # Use Swagger UI version 3
}
swagger = Swagger(app)


### Function to Determine Address Type ###
def is_url_or_ip(address):
    """
    Distinguish between a URL (domain) and an IP address.
    """
    # Regex for IPv4 address
    ipv4_pattern = re.compile(r'^(\d{1,3}\.){3}\d{1,3}$')

    # Regex for IPv6 address
    ipv6_pattern = re.compile(r'^([0-9a-fA-F]{1,4}:){7,7}[0-9a-fA-F]{1,4}|'
                              r'([0-9a-fA-F]{1,4}:){1,7}:|'
                              r'([0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|'
                              r'([0-9a-fA-F]{1,4}:){1,5}(:[0-9a-fA-F]{1,4}){1,2}|'
                              r'([0-9a-fA-F]{1,4}:){1,4}(:[0-9a-fA-F]{1,4}){1,3}|'
                              r'([0-9a-fA-F]{1,4}:){1,3}(:[0-9a-fA-F]{1,4}){1,4}|'
                              r'([0-9a-fA-F]{1,4}:){1,2}(:[0-9a-fA-F]{1,4}){1,5}|'
                              r'[0-9a-fA-F]{1,4}:((:[0-9a-fA-F]{1,4}){1,6})|'
                              r':((:[0-9a-fA-F]{1,4}){1,7}|:)|'
                              r'fe80:(:[0-9a-fA-F]{0,4}){0,4}%[0-9a-zA-Z]{1,}|'
                              r'::(ffff(:0{1,4}){0,1}:){0,1}'
                              r'((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}'
                              r'(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])|'
                              r'([0-9a-fA-F]{1,4}:){1,4}:'
                              r'((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}'
                              r'(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])$')

    # Regex for domain names
    domain_pattern = re.compile(
        r'^(?!-)[A-Za-z0-9-]{1,63}(?<!-)(\.[A-Za-z]{2,6})+$'
    )

    # Check if it's IPv4
    if ipv4_pattern.match(address):
        return 'ip'

    # Check if it's IPv6
    elif ipv6_pattern.match(address):
        return 'ip'

    # Check if it's a domain
    elif domain_pattern.match(address):
        return 'url'

    # If it ends with .onion
    elif address.endswith(".onion"):
        return 'onion'

    return 'unknown'


### Sorting Priority ###
def sort_priority(host):
    """
    Define sorting priority based on address type.
    """
    address_type = is_url_or_ip(host)
    if address_type == 'url':
        return (0, host)  # Highest priority
    elif address_type == 'onion':
        return (1, host)  # Medium priority
    elif address_type == 'ip':
        return (2, host)  # Lowest priority
    return (3, host)  # Unknown (lowest priority)




def query_electrumx_server(host, ports, method=None, params=[]):
    # Use a default method if none is provided
    if not method:
        method = "blockchain.headers.subscribe"

    methods = [
        {"method": method, "params": params},
        {"method": "server.features", "params": []},
        {"method": "blockchain.numblocks.subscribe", "params": []},
    ]

    connection_options = []
    if isinstance(ports, dict):
        if "s" in ports:
            connection_options.append({"port": int(ports["s"]), "use_ssl": True})
        if "t" in ports:
            connection_options.append({"port": int(ports["t"]), "use_ssl": False})
    else:
        connection_options.append({"port": int(ports), "use_ssl": False})

    # Check if the host is a .onion address
    use_tor_proxy = host.endswith(".onion")

    # First, check server reachability
    reachable = False
    for connection in connection_options:
        try:
            if use_tor_proxy:
                # Set up a socket using the Tor proxy
                socks.set_default_proxy(socks.SOCKS5, TOR_PROXY_HOST, TOR_PROXY_PORT)
                socket.socket = socks.socksocket

            # Attempt to connect to the server
            with socket.create_connection((host, connection["port"]), timeout=5):
                reachable = True
                break  # Exit loop if one connection succeeds
        except Exception as e:
            logging.error(f"Error connecting to {host}:{connection['port']}: {e}")
        finally:
            if use_tor_proxy:
                # Reset the proxy and restore default socket behavior
                socks.set_default_proxy(None)
                socket.socket = socket.create_connection

    if not reachable:
        return {
            "error": "Server is unreachable",
            "host": host,
            "ports": ports,
        }

    # Proceed with querying the server if reachable
    for connection in connection_options:
        for method_config in methods:
            try:
                request_data = json.dumps({
                    "id": 1,
                    "method": method_config["method"],
                    "params": method_config["params"]
                }) + "\n"

                start_time = time.time()

                if use_tor_proxy:
                    # Set up a socket using the Tor proxy
                    socks.set_default_proxy(socks.SOCKS5, TOR_PROXY_HOST, TOR_PROXY_PORT)
                    socket.socket = socks.socksocket

                with socket.create_connection((host, connection["port"]), timeout=10) as sock:
                    if connection["use_ssl"]:
                        context = ssl.create_default_context()
                        context.check_hostname = False
                        context.verify_mode = ssl.CERT_NONE
                        with context.wrap_socket(sock, server_hostname=host) as ssock:
                            ssock.sendall(request_data.encode())
                            response = ssock.recv(4096).decode()
                            self_signed = True
                    else:
                        sock.sendall(request_data.encode())
                        response = sock.recv(4096).decode()
                        self_signed = False

                    response_data = json.loads(response)
                    ping_time = round((time.time() - start_time) * 1000, 2)

                    if "result" not in response_data:
                        return {
                            "ping": ping_time,
                            "error": "Malformed response: Missing 'result' field",
                            "response_id": response_data.get("id"),
                            "response_error": response_data.get("error"),
                            "method_used": method_config["method"],
                            "connection_type": "SSL" if connection["use_ssl"] else "Plaintext",
                            "self_signed": self_signed
                        }

                    return {
                        "ping": ping_time,
                        "result": response_data.get("result"),
                        "method_used": method_config["method"],
                        "connection_type": "SSL" if connection["use_ssl"] else "Plaintext",
                        "self_signed": self_signed
                    }

            except ssl.SSLError as e:
                logging.error(f"SSL error on {host}:{connection['port']} using {method_config['method']}: {e}")
                continue
            except Exception as e:
                logging.error(f"Error on {host}:{connection['port']} using {method_config['method']}: {e}")
                continue
            finally:
                if use_tor_proxy:
                    # Reset the proxy and restore default socket behavior
                    socks.set_default_proxy(None)
                    socket.socket = socket.create_connection

    return {
        "error": "All methods failed or server is unreachable",
        "host": host,
        "ports": ports
    }



def resolve_hostname_to_ips(hostname):
    if hostname.endswith(".onion"):
        # Skip resolution for .onion addresses
        return []

    try:
        addresses = socket.getaddrinfo(hostname, None)
        ip_addresses = {result[4][0] for result in addresses}  # Extract unique IPs
        print(f"{hostname} resolved to: {', '.join(ip_addresses)}")
        return list(ip_addresses)
    except socket.gaierror as e:
        print(f"Error resolving {hostname}: {e}")
        return []



def parse_block_header(header_hex):
    # Convert hex to bytes
    header = bytes.fromhex(header_hex)

    # Unpack the header fields
    version = struct.unpack('<I', header[0:4])[0]
    prev_block = header[4:36][::-1].hex()  # Reverse bytes for little-endian
    merkle_root = header[36:68][::-1].hex()  # Reverse bytes for little-endian
    timestamp = struct.unpack('<I', header[68:72])[0]
    bits = struct.unpack('<I', header[72:76])[0]
    nonce = struct.unpack('<I', header[76:80])[0]

    # Convert timestamp to human-readable date
    timestamp_human = datetime.datetime.utcfromtimestamp(timestamp)

    # Return parsed data
    return {
        "version": version,
        "prev_block": prev_block,
        "merkle_root": merkle_root,
        "timestamp": timestamp,
        "timestamp_human": timestamp_human,
        "bits": bits,
        "nonce": nonce
    }

### API Endpoints ###

@app.route('/electrum/servers', methods=['GET'])
def servers():
    """
    Fetch Electrum server list with streamlined structure.
    ---
    responses:
      200:
        description: List of Electrum servers with version numbers.
        schema:
          type: object
          properties:
            servers:
              type: object
              description: "JSON response containing server information."
      500:
        description: "Error while fetching servers."
    """
    try:
        # Execute 'electrum getservers' command
        result = subprocess.run(
            ["electrum", "getservers"],
            capture_output=True,
            text=True,
            check=True
        )
        servers = json.loads(result.stdout)  # Parse JSON output

        # Sort servers based on priority
        sorted_servers = {
            host: {
                "pruning": details.get("pruning", "-"),
                "s": details.get("s", None),
                "t": details.get("t", None),
                "version": details.get("version", "unknown"),
            }
            for host, details in sorted(servers.items(), key=lambda x: sort_priority(x[0]))
        }

        return jsonify({"servers": sorted_servers})

    except Exception as e:
        return jsonify({"error": str(e)}), 500


@app.route('/electrum/query', methods=['GET'])
def electrum_query():
    """
    Query an ElectrumX server with enhanced version information.
    ---
    parameters:
      - name: url
        in: query
        type: string
        required: true
        description: "Electrum server hostname or IP address."
      - name: port
        in: query
        type: integer
        required: false
        default: 50002
        description: "Server port number (default SSL 50002)."
    responses:
      200:
        description: Flattened server response with host and resolved IPs.
        schema:
          type: object
          properties:
            host:
              type: string
              description: "The hostname provided."
            resolved_ips:
              type: array
              items:
                type: string
              description: "List of resolved IP addresses for the hostname."
            ping:
              type: number
              description: "Ping time in milliseconds."
            block_version:
              type: string
              description: "Renamed block version from the response."
            <other_flattened_fields>:
              type: any
              description: "Flattened fields from the original response."
      400:
        description: "Invalid input."
    """
    # Get query parameters
    server_url = request.args.get('url', '')
    port = int(request.args.get('port', 50002))  # Default SSL port 50002

    # Validate input
    if not server_url:
        return jsonify({"error": "Server URL is required"}), 400

    # Resolve hostname to IPs
    resolved_ips = resolve_hostname_to_ips(server_url)

    # Query the ElectrumX server
    result = query_electrumx_server(server_url, {"s": port}, None)

    # Log the raw result returned by the server
    logging.info(f"Result received from the server: {result}")

    # Flatten the nested result
    flattened_result = {
        "host": server_url,
        "resolved_ips": resolved_ips,
        "ping": result.get("ping"),
        "method_used": result.get("method_used"),
        "connection_type": result.get("connection_type"),
        "self_signed": result.get("self_signed"),
        "version": result.get("version", "unknown"),
        "error": result.get("error", ""),
    }

    if "result" in result:
        # Flatten nested 'result' fields
        flattened_result.update(result["result"])

        # Check if 'hex' key is present and parse it
        if "hex" in result["result"]:
            try:
                parsed_hex = parse_block_header(result["result"]["hex"])
                flattened_result.update(parsed_hex)  # Include parsed hex data
                flattened_result.pop("hex")
            except Exception as e:
                logging.error(f"Error parsing hex: {e}")
                flattened_result["hex_parse_error"] = str(e)  # Handle hex parsing errors

    return jsonify(flattened_result)



if __name__ == '__main__':
    # Run Flask app on all available interfaces
    app.run(host="0.0.0.0", port=5000, debug=True)



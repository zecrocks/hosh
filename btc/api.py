from flask import Flask, request, jsonify
from flasgger import Swagger
import json
import subprocess
import socket
import ssl
import time
import re

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


def query_electrumx_server(host, ports, method, params=[]):
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

    for connection in connection_options:
        for method_config in methods:
            try:
                request_data = json.dumps({
                    "id": 1,
                    "method": method_config["method"],
                    "params": method_config["params"]
                }) + "\n"

                start_time = time.time()
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
                print(f"SSL error on {host}:{connection['port']} using {method_config['method']}: {e}")
                continue
            except Exception as e:
                print(f"Error on {host}:{connection['port']} using {method_config['method']}: {e}")
                continue

    return {
        "error": "All methods failed or server is unreachable",
        "host": host,
        "ports": ports
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
      - name: method
        in: query
        type: string
        required: false
        default: blockchain.headers.subscribe
        description: "JSON-RPC method to call."
    responses:
      200:
        description: Server response with version, ping time, and result.
        schema:
          type: object
          properties:
            ping:
              type: number
              description: "Ping time in milliseconds."
            response:
              type: object
              description: "JSON response from Electrum server."
            version:
              type: string
              description: "Server version if available."
            method_used:
              type: string
              description: "JSON-RPC method used to obtain the result."
      400:
        description: "Invalid input."
    """
    # Get query parameters
    server_url = request.args.get('url', '')
    method = request.args.get('method', 'blockchain.headers.subscribe')
    port = int(request.args.get('port', 50002))  # Default SSL port 50002

    # Validate input
    if not server_url:
        return jsonify({"error": "Server URL is required"}), 400

    # Query the server features for version
    version_info = {}
    try:
        version_result = query_electrumx_server(server_url, {"s": port}, "server.features")
        version_info = version_result.get("response", {}).get("result", {})
    except Exception as e:
        print(f"Failed to fetch version info for {server_url}: {e}")

    # Query the ElectrumX server for the requested method
    result = query_electrumx_server(server_url, {"s": port}, method)

    # Add version information to the result
    result["version"] = version_info.get("server_version", "unknown")

    return jsonify(result)


if __name__ == '__main__':
    # Run Flask app on all available interfaces
    app.run(host="0.0.0.0", port=5000, debug=True)



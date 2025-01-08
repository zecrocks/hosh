from flask import Flask, request, jsonify
from flasgger import Swagger
import json
import subprocess
import socket
import ssl
import time

app = Flask(__name__)

# Configure Swagger UI
app.config['SWAGGER'] = {
    'title': 'BTC worker',  # Set the tab title
    'uiversion': 3  # Use Swagger UI version 3
}
swagger = Swagger(app)

### ElectrumX Query Function ###
def query_electrumx_server(host, port, method, params=[]):
    try:
        # Prepare JSON-RPC request
        request_data = json.dumps({
            "id": 1,
            "method": method,
            "params": params
        }) + "\n"

        # Create SSL context for secure connection
        context = ssl.create_default_context()

        # Measure ping time
        start_time = time.time()

        # Connect to the ElectrumX server
        with socket.create_connection((host, port), timeout=10) as sock:
            with context.wrap_socket(sock, server_hostname=host) as ssock:
                # Send request
                ssock.sendall(request_data.encode())

                # Receive and parse response
                response = ssock.recv(4096).decode()
                response_data = json.loads(response)

        # Calculate ping time
        ping_time = round((time.time() - start_time) * 1000, 2)  # in milliseconds

        return {"ping": ping_time, "response": response_data}

    except Exception as e:
        return {"error": str(e)}


### API Endpoints ###

@app.route('/electrum/servers', methods=['GET'])
def servers():
    """
    Fetch Electrum server list
    ---
    responses:
      200:
        description: List of Electrum servers.
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

        return jsonify({"servers": servers})

    except Exception as e:
        return jsonify({"error": str(e)}), 500


@app.route('/electrum/query', methods=['GET'])
def electrum_query():
    """
    Query an ElectrumX server
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
        description: Server response with ping time and result.
        schema:
          type: object
          properties:
            ping:
              type: number
              description: "Ping time in milliseconds."
            response:
              type: object
              description: "JSON response from Electrum server."
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

    # Query the ElectrumX server
    result = query_electrumx_server(server_url, port, method)
    return jsonify(result)


if __name__ == '__main__':
    # Run Flask app on all available interfaces
    app.run(host="0.0.0.0", port=5000, debug=True)



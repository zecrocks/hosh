use axum::{
    extract::Query,
    response::Json,
    routing::get,
    Router,
};
use bitcoin::{
    blockdata::block::Header as BlockHeader,
    consensus::encode::{deserialize, serialize},
};
use chrono::{TimeZone, Utc};
use electrum_client::{Client, ElectrumApi};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    net::SocketAddr,
    time::Instant,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_native_tls::TlsConnector;


// Query parameters for the /electrum/peers endpoint
#[derive(Deserialize)]
struct PeerQueryParams {
    url: String,
    port: Option<u16>,
}

async fn fetch_peers(host: &str, port: u16) -> Result<Vec<Value>, String> {
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(addr).await.map_err(|e| {
        format!("Failed to connect to {}:{} - {}", host, port, e)
    })?;
    println!("Connected to {}:{}", host, port);

    let tls_connector =
        TlsConnector::from(native_tls::TlsConnector::new().map_err(|e| e.to_string())?);
    let mut stream = tls_connector.connect(host, stream).await.map_err(|e| {
        format!("Failed to establish TLS connection to {}:{} - {}", host, port, e)
    })?;
    println!("TLS connection established to {}:{}", host, port);

    let request = json!({
        "id": 1,
        "method": "server.peers.subscribe",
        "params": []
    });
    let request_str = serde_json::to_string(&request).unwrap() + "\n";
    stream.write_all(request_str.as_bytes()).await.map_err(|e| {
        format!("Failed to send request to {}:{} - {}", host, port, e)
    })?;
    println!("Request sent: {}", request_str);

    let mut response_str = String::new();
    let mut buffer = vec![0; 4096];
    loop {
        let n = stream.read(&mut buffer).await.map_err(|e| {
            format!("Failed to read response from {}:{} - {}", host, port, e)
        })?;
        if n == 0 {
            break;
        }
        response_str.push_str(&String::from_utf8_lossy(&buffer[..n]));
        if response_str.ends_with("\n") {
            break;
        }
    }
    println!("Response received: {}", response_str);

    let response: Value = serde_json::from_str(&response_str).map_err(|e| {
        format!("Failed to parse JSON response from {}:{} - {}", host, port, e)
    })?;
    if let Some(peers) = response["result"].as_array() {
        Ok(peers.clone())
    } else {
        Err(format!("No peers found in response from {}:{}", host, port))
    }
}
async fn electrum_peers(Query(params): Query<PeerQueryParams>) -> Result<Json<serde_json::Value>, String> {
    let host = params.url;
    let port = params.port.unwrap_or(50002);

    // Fetch peers from the specified host and port
    let peers = fetch_peers(&host, port).await?;

    let mut peers_map = serde_json::Map::new();

    for peer in peers {
        if let Some(peer_details) = peer.as_array() {
            let address = peer_details.get(0).and_then(|v| v.as_str()).unwrap_or("Unknown");
            let _hostname = peer_details.get(1).and_then(|v| v.as_str()).unwrap_or("Unknown");

            let features = peer_details
                .get(2)
                .and_then(|v| v.as_array())
                .unwrap_or_else(|| {
                    static EMPTY_VEC: Vec<Value> = Vec::new();
                    &EMPTY_VEC
                })
                .iter()
                .filter_map(|f| f.as_str())
                .collect::<Vec<&str>>();



            let version = features.iter().find_map(|f| f.strip_prefix('v')).unwrap_or("unknown");

            // Construct the peer entry
            let peer_entry = serde_json::json!({
                "pruning": "-",  // Placeholder, as pruning information is not provided by peers
                "s": if features.iter().any(|&f| f.starts_with("s50002")) {
                    Some("50002".to_string())
                } else {
                    None
                },
                "t": if features.iter().any(|&f| f.starts_with("t50001")) {
                    Some("50001".to_string())
                } else {
                    None
                },
                "version": version,
            });

            peers_map.insert(address.to_string(), peer_entry);
        }
    }

    Ok(Json(serde_json::json!({ "peers": peers_map })))
}



fn parse_block_header(header_hex: &str) -> Result<serde_json::Value, String> {
    let header_bytes = hex::decode(header_hex).map_err(|e| e.to_string())?;
    let header: BlockHeader = deserialize(&header_bytes).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "version": header.version,
        "prev_blockhash": header.prev_blockhash.to_string(),
        "merkle_root": header.merkle_root.to_string(),
        "time": header.time,
        "bits": header.bits,
        "nonce": header.nonce as u32  // Explicitly cast to u32
    }))
}


#[derive(Serialize)]
struct HealthCheckResponse {
    status: String,
    components: HashMap<String, String>,
}

#[derive(Deserialize)]
struct QueryParams {
    url: String,
    port: Option<u16>,
}


#[derive(Serialize)]
struct _FlattenedResponse {
    host: String,
    resolved_ips: Vec<String>,
    ping: Option<u64>,
    method_used: Option<String>,
    error: Option<String>,
}

async fn health_check() -> Json<HealthCheckResponse> {
    let mut components = HashMap::new();
    components.insert("electrum".to_string(), "healthy".to_string());
    let response = HealthCheckResponse {
        status: "healthy".to_string(),
        components,
    };
    Json(response)
}

async fn electrum_servers() -> Result<Json<serde_json::Value>, String> {
    let url = "https://raw.githubusercontent.com/spesmilo/electrum/refs/heads/master/electrum/servers.json";

    let http_client = HttpClient::new();
    let response = http_client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch server list: {}", e))?
        .json::<HashMap<String, HashMap<String, serde_json::Value>>>()
        .await
        .map_err(|e| format!("Failed to parse server list JSON: {}", e))?;

    let mut servers = serde_json::Map::new();

    for (host, details) in response {
        // Extract ports as strings directly
        let s_port = details.get("s").cloned().unwrap_or(serde_json::Value::Null);
        let t_port = details.get("t").cloned().unwrap_or(serde_json::Value::Null);

        // Extract other fields
        let version = details
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let pruning = details
            .get("pruning")
            .and_then(|v| v.as_str())
            .unwrap_or("-")
            .to_string();

        let server_entry = serde_json::json!({
            "pruning": pruning,
            "s": s_port,
            "t": t_port,
            "version": version,
        });

        servers.insert(host, server_entry);
    }

    Ok(Json(serde_json::json!({ "servers": servers })))
}




async fn electrum_query(
    Query(params): Query<QueryParams>,
) -> Result<Json<serde_json::Value>, String> {
    // Extract the host and port from query parameters
    let host = params.url;
    let port = params.port.unwrap_or(50002);

    // Construct the Electrum server address
    let server_addr = format!("ssl://{}:{}", host, port);
    
    // Start measuring ping time
    let start_time = Instant::now();
    
    // Instantiate a new Electrum client for the provided server
    let client = Client::new(&server_addr)
        .map_err(|e| format!("Failed to connect to Electrum server ({}): {}", server_addr, e))?;

    // Calculate ping in milliseconds
    let ping = start_time.elapsed().as_millis() as f64;

    // Execute the blockchain.headers.subscribe method
    let header_subscribe_result = client.block_headers_subscribe()
        .map_err(|e| format!("block_headers_subscribe failed ({}): {}", server_addr, e.to_string()))?;

    // Serialize the block header to a hexadecimal string
    let block_header_hex = hex::encode(serialize(&header_subscribe_result.header));
    
    // Parse the block header for detailed information
    let parsed_header = parse_block_header(&block_header_hex).unwrap_or(serde_json::json!({
        "error": "Failed to parse block header"
    }));
    
    // Resolve IP addresses for the host
    let resolved_ips = match tokio::net::lookup_host(format!("{}:{}", host, port)).await {
        Ok(addrs) => addrs.map(|addr| addr.ip().to_string()).collect::<Vec<String>>(),
        Err(_) => vec![],
    };
    
    // Convert timestamp to human-readable format
    let timestamp = parsed_header["time"]
        .as_i64()
        .unwrap_or(0);
    let timestamp_human = Utc.timestamp_opt(timestamp, 0)
        .single()
        .ok_or_else(|| "Invalid timestamp".to_string())?
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    
    // Construct the response
    let response = serde_json::json!({
        "host": host,
        "method_used": "blockchain.headers.subscribe",
        "height": header_subscribe_result.height,
        "bits": parsed_header["bits"],
        "merkle_root": parsed_header["merkle_root"],
        "nonce": parsed_header["nonce"],
        "prev_block": parsed_header["prev_blockhash"], // Renamed from "prev_blockhash"
        "ping": ping,
        "resolved_ips": resolved_ips,
        "self_signed": true, // Placeholder: customize based on SSL validation
        "timestamp": parsed_header["time"],
        "timestamp_human": timestamp_human,
        "version": parsed_header["version"],
        "connection_type": "SSL",
        "error": ""
    });
    
    Ok(Json(response))
}

#[derive(Serialize)]
struct ApiDescription {
    description: String,
    endpoints: Vec<EndpointInfo>,
}

#[derive(Serialize)]
struct EndpointInfo {
    method: String,
    path: String,
    description: String,
    example_response: serde_json::Value,
}

async fn api_info() -> Json<ApiDescription> {
    let api_info = ApiDescription {
        description: "This is an Electrum-based API.".to_string(),
        endpoints: vec![
            EndpointInfo {
                method: "GET".to_string(),
                path: "/".to_string(),
                description: "Provides information about this API.".to_string(),
                example_response: serde_json::json!({
                    "description": "This is an Electrum-based API.",
                    "endpoints": [
                        {
                            "method": "GET",
                            "path": "/",
                            "description": "Provides information about this API."
                        }
                    ]
                }),
            },
            EndpointInfo {
                method: "GET".to_string(),
                path: "/healthz".to_string(),
                description: "Checks the health of the service.".to_string(),
                example_response: serde_json::json!({
                    "status": "healthy",
                    "components": {
                        "electrum": "healthy"
                    }
                }),
            },
            EndpointInfo {
                method: "GET".to_string(),
                path: "/electrum/servers".to_string(),
                description: "Fetches the list of Electrum servers.".to_string(),
                example_response: serde_json::json!({
                    "servers": {
                        "104.198.149.61": {
                            "pruning": "-",
                            "s": "50002",
                            "t": "50001",
                            "version": "1.4.2",
                            "peer_count": 42
                        },
                        "128.0.190.26": {
                            "pruning": "-",
                            "s": "50002",
                            "t": null,
                            "version": "1.4.2",
                            "peer_count": 0
                        }
                    }
                }),
            },
            EndpointInfo {
                method: "GET".to_string(),
                path: "/electrum/peers".to_string(),
                description: "Fetches the list of peers from a specific Electrum server.".to_string(),
                example_response: serde_json::json!({
                    "peers": {
                        "45.154.252.100": {
                            "pruning": "-",
                            "s": "50002",
                            "t": null,
                            "version": "1.5"
                        },
                        "135.181.215.237": {
                            "pruning": "-",
                            "s": "50002",
                            "t": "50001",
                            "version": "1.4"
                        },
                        "unknown.onion": {
                            "pruning": "-",
                            "s": null,
                            "t": "50001",
                            "version": "1.5"
                        }
                    }
                }),
            },
            EndpointInfo {
                method: "GET".to_string(),
                path: "/electrum/query".to_string(),
                description: "Queries blockchain headers for a specific server.".to_string(),
                example_response: serde_json::json!({
                    "bits": 386043996,
                    "connection_type": "SSL",
                    "error": "",
                    "height": 878812,
                    "host": "electrum.blockstream.info",
                    "merkle_root": "9c37963b9e67a138ef18595e21eae9b5517abdaf4f500584ac88c2a7d15589a7",
                    "method_used": "blockchain.headers.subscribe",
                    "nonce": 4216690212u32,
                    "ping": 157.55,
                    "prev_block": "00000000000000000000bd9001ebe6182a864943ce8b04338b81986ee2b0ebf3",
                    "resolved_ips": [
                        "34.36.93.230"
                    ],
                    "self_signed": true,
                    "timestamp": 1736622010,
                    "timestamp_human": "Sat, 11 Jan 2025 19:00:10 GMT",
                    "version": 828039168
                }),
            },
        ],
    };
    Json(api_info)
}



#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(api_info))
        .route("/healthz", get(health_check))
        .route("/electrum/servers", get(electrum_servers))
        .route("/electrum/query", get(electrum_query))
        .route("/electrum/peers", get(electrum_peers)); // Add this line

    let addr = SocketAddr::from(([0, 0, 0, 0], 5000));
    println!("Server running on http://{}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}


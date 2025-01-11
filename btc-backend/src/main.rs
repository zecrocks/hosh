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
use std::collections::HashMap;
use std::{
    net::SocketAddr,
    sync::Arc,
    time::Instant,
};
use tokio::sync::Mutex;


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


#[derive(Clone)]
struct AppState {
    electrum_client: Arc<Mutex<Client>>,
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
struct ServerResponse {
    host: String,
    ports: HashMap<String, Option<u16>>,
    version: String,
}


#[derive(Serialize)]
struct _FlattenedResponse {
    host: String,
    resolved_ips: Vec<String>,
    ping: Option<u64>,
    method_used: Option<String>,
    error: Option<String>,
}

async fn health_check(_state: Arc<AppState>) -> Json<HealthCheckResponse> {
    let mut components = HashMap::new();
    components.insert("electrum".to_string(), "healthy".to_string());
    let response = HealthCheckResponse {
        status: "healthy".to_string(),
        components,
    };
    Json(response)
}


async fn get_servers(_state: Arc<AppState>) -> Result<Json<serde_json::Value>, String> {
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
        let s_port = details
            .get("s")
            .and_then(|v| v.as_u64())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "null".to_string());

        let t_port = details
            .get("t")
            .and_then(|v| v.as_u64())
            .map(|t| t.to_string())
            .unwrap_or_else(|| "null".to_string());

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
            "s": if s_port == "null" { serde_json::Value::Null } else { serde_json::Value::String(s_port) },
            "t": if t_port == "null" { serde_json::Value::Null } else { serde_json::Value::String(t_port) },
            "version": version
        });

        servers.insert(host, server_entry);
    }

    Ok(Json(serde_json::json!({ "servers": servers })))
}




async fn electrum_query(
    Query(params): Query<QueryParams>,
    state: Arc<AppState>,
) -> Result<Json<serde_json::Value>, String> {
    let client = state.electrum_client.lock().await;

    // Extract the port, defaulting to 50002 if not provided
    let port = params.port.unwrap_or(50002);

    // Start measuring ping time
    let start_time = Instant::now();

    // Execute the blockchain.headers.subscribe method
    let header_subscribe_result = client.block_headers_subscribe()
        .map_err(|e| e.to_string())?;

    // Calculate ping
    let ping = start_time.elapsed().as_millis() as f64;

    // Serialize the block header to a hexadecimal string
    let block_header_hex = hex::encode(serialize(&header_subscribe_result.header));

    // Parse the block header for detailed information
    let parsed_header = parse_block_header(&block_header_hex).unwrap_or(serde_json::json!({
        "error": "Failed to parse block header"
    }));

    // Resolve IP addresses for the host
    let resolved_ips = match tokio::net::lookup_host(format!("{}:{}", params.url, port)).await {
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
        "host": params.url,
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
                            "version": "1.4.2"
                        },
                        "104.248.139.211": {
                            "pruning": "-",
                            "s": "50002",
                            "t": "50001",
                            "version": "1.4.2"
                        },
                        "128.0.190.26": {
                            "pruning": "-",
                            "s": "50002",
                            "t": null,
                            "version": "1.4.2"
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
                    "nonce": 4216690212u32,  // Annotate as u32
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

// Main function and router setup remain the same.
#[tokio::main]
async fn main() {
    let electrum_client = Client::new("ssl://electrum.blockstream.info:50002")
        .expect("Failed to connect to Electrum server");

    let state = Arc::new(AppState {
        electrum_client: Arc::new(Mutex::new(electrum_client)),
    });

    let app = Router::new()
        .route("/", get(api_info))
        .route("/healthz", get({
            let state = Arc::clone(&state);
            move || health_check(state)
        }))
        .route("/electrum/servers", get({
            let state = Arc::clone(&state);
            move || async { get_servers(state).await }
        }))
        .route("/electrum/query", get({
            let state = Arc::clone(&state);
            move |query| electrum_query(query, state)
        }));

    let addr = SocketAddr::from(([0, 0, 0, 0], 5000));
    println!("Server running on http://{}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}


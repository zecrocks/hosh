use axum::{
    routing::{get},
    extract::Query,
    response::Json,
    Router,
};
use electrum_client::{Client, ElectrumApi};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;

use bitcoin::blockdata::block::Header as BlockHeader;
use bitcoin::consensus::encode::deserialize;
use bitcoin::consensus::encode::serialize;


fn parse_block_header(header_hex: &str) -> Result<serde_json::Value, String> {
    let header_bytes = hex::decode(header_hex).map_err(|e| e.to_string())?;
    let header: BlockHeader = deserialize(&header_bytes).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "version": header.version,
        "prev_blockhash": header.prev_blockhash.to_string(),
        "merkle_root": header.merkle_root.to_string(),
        "time": header.time,
        "bits": header.bits,
        "nonce": header.nonce
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


async fn get_servers(state: Arc<AppState>) -> Result<Json<Vec<ServerResponse>>, String> {
    let client = state.electrum_client.lock().await;

    // Use raw_call to query peers
    let peers: serde_json::Value = client
        .raw_call("server.peers.subscribe", vec![])
        .map_err(|e| e.to_string())?;

    let mut servers = Vec::new();

    // Parse the returned JSON
    if let Some(peer_list) = peers.as_array() {
        for peer in peer_list {
            if let Some(host) = peer.get(1).and_then(|h| h.as_str()) {
                let mut ports = HashMap::new();

                // Extract SSL and TCP ports
                if let Some(port_info) = peer.get(2).and_then(|p| p.as_object()) {
                    if let Some(ssl_port) = port_info.get("s").and_then(|p| p.as_u64()) {
                        ports.insert("s".to_string(), Some(ssl_port as u16));
                    }
                    if let Some(tcp_port) = port_info.get("t").and_then(|p| p.as_u64()) {
                        ports.insert("t".to_string(), Some(tcp_port as u16));
                    }
                }

                servers.push(ServerResponse {
                    host: host.to_string(),
                    ports,
                    version: "unknown".to_string(), // Replace with actual version if available
                });
            }
        }
    }

    Ok(Json(servers))
}


use std::time::Instant;

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


    use chrono::{TimeZone, Utc};

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


#[tokio::main]
async fn main() {
    let electrum_client = Client::new("ssl://electrum.blockstream.info:50002")
        .expect("Failed to connect to Electrum server");

    let state = Arc::new(AppState {
        electrum_client: Arc::new(Mutex::new(electrum_client)),
    });

    let app = Router::new()
        .route("/healthz", get({
            let state = Arc::clone(&state);
            move || health_check(state)
        }))
        .route("/electrum/servers", get({
            let state = Arc::clone(&state);
            move || get_servers(state)
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


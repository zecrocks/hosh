use axum::{extract::Query, response::Json};
use electrum_client::{Client as ElectrumClient, ElectrumApi};
use reqwest::Client as HttpClient;
use serde::{Deserialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_native_tls::TlsConnector;
use bitcoin::blockdata::block::Header as BlockHeader;
use bitcoin::consensus::encode::deserialize;
use tracing::{error, warn};


fn parse_block_header(header_hex: &str) -> Result<serde_json::Value, String> {
    let header_bytes = hex::decode(header_hex).map_err(|e| e.to_string())?;
    let header: BlockHeader = deserialize(&header_bytes).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "version": header.version,
        "prev_blockhash": header.prev_blockhash.to_string(),
        "merkle_root": header.merkle_root.to_string(),
        "time": header.time,
        "bits": header.bits,
        "nonce": header.nonce as u32
    }))
}


/// Query parameters for the `/electrum/peers` route
#[derive(Deserialize)]
pub struct PeerQueryParams {
    pub url: String,
    pub port: Option<u16>,
}

/// Enum to represent a connection type
enum Connection {
    Tcp(TcpStream),
    Tls(tokio_native_tls::TlsStream<TcpStream>),
}

/// Attempt to establish a connection
async fn try_connect(
    host: &str,
    port: u16,
    use_ssl: bool,
) -> Result<(bool, Connection), String> {
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(&addr).await.map_err(|e| {
        format!("Failed to connect to {}:{} - {}", host, port, e)
    })?;

    if use_ssl {
        let tls_connector = TlsConnector::from(
            native_tls::TlsConnector::new().map_err(|e| e.to_string())?,
        );

        match tls_connector.connect(host, stream).await {
            Ok(tls_stream) => Ok((false, Connection::Tls(tls_stream))),
            Err(_) => {
                let tls_connector = TlsConnector::from(
                    native_tls::TlsConnector::builder()
                        .danger_accept_invalid_certs(true)
                        .build()
                        .map_err(|e| e.to_string())?,
                );
                tls_connector
                    .connect(host, TcpStream::connect(&addr).await.map_err(|e| {
                        format!("Failed to reconnect ignoring cert errors: {}", e)
                    })?)
                    .await
                    .map(|tls_stream| (true, Connection::Tls(tls_stream)))
                    .map_err(|e| format!("Failed to connect with SSL ignoring cert errors: {}", e))
            }
        }
    } else {
        Ok((false, Connection::Tcp(stream)))
    }
}



pub async fn electrum_servers() -> Result<Json<serde_json::Value>, String> {
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



#[derive(Deserialize)]
pub struct QueryParams {
    pub url: String,
    pub port: Option<u16>,
}



pub async fn electrum_query(Query(params): Query<QueryParams>) -> Result<Json<serde_json::Value>, axum::response::Response> {
    fn error_response(message: &str) -> axum::response::Response {
        let error_body = serde_json::json!({
            "error": message
        });
        axum::response::Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .body(axum::body::boxed(axum::body::Full::from(error_body.to_string())))
            .unwrap()
    }

    let host = &params.url;
    let port = params.port.unwrap_or(50002);

    let (self_signed, _connection) = match try_connect(host, port, true).await {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!("SSL connection failed: {}", e);
            match try_connect(host, 50001, false).await {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!("TCP connection failed: {}", e);
                    return Err(error_response(&format!(
                        "Failed to connect to {}: {}", host, e
                    )));
                },
            }
        },
    };

    let client = match ElectrumClient::new(&format!("ssl://{}:{}", host, port)) {
        Ok(client) => client,
        Err(e) => {
            tracing::error!("Failed to create Electrum client: {}", e);
            return Err(error_response(&format!(
                "Failed to create Electrum client: {}", e
            )));
        },
    };

    let resolved_ips = match tokio::net::lookup_host(format!("{}:{}", host, port)).await {
        Ok(addrs) => addrs.map(|addr| addr.ip().to_string()).collect::<Vec<String>>(),
        Err(e) => {
            tracing::warn!("Failed to resolve IPs for {}: {}", host, e);
            vec![]
        },
    };

    let start_time = Instant::now();

    // Attempt `blockchain.headers.subscribe`
    if let Ok(response) = client.raw_call("blockchain.headers.subscribe", Vec::new()) {
        let ping = start_time.elapsed().as_millis() as f64;

        let bits = response.get("bits").cloned().unwrap_or(serde_json::Value::Null);
        let block_height = response.get("block_height").cloned().unwrap_or(serde_json::Value::Null);
        let merkle_root = response.get("merkle_root").cloned().unwrap_or(serde_json::Value::Null);
        let nonce = response.get("nonce").cloned().unwrap_or(serde_json::Value::Null);
        let prev_block_hash = response.get("prev_block_hash").cloned().unwrap_or(serde_json::Value::Null);
        let timestamp = response.get("timestamp").cloned().unwrap_or(serde_json::Value::Null);
        let version = response.get("version").cloned().unwrap_or(serde_json::Value::Null);

        return Ok(Json(serde_json::json!({
            "bits": bits,
            "block_height": block_height,
            "connection_type": if self_signed { "SSL (self-signed)" } else { "SSL" },
            "error": "",
            "host": host,
            "merkle_root": merkle_root,
            "method_used": "blockchain.headers.subscribe",
            "nonce": nonce,
            "ping": ping,
            "prev_block_hash": prev_block_hash,
            "resolved_ips": resolved_ips,
            "self_signed": self_signed,
            "timestamp": timestamp,
            "version": version
        })));
    }

    // Attempt `server.features`
    if let Ok(response) = client.raw_call("server.features", Vec::new()) {
        let ping = start_time.elapsed().as_millis() as f64;
        return Ok(Json(serde_json::json!({
            "host": host,
            "is_online": true,
            "self_signed": self_signed,
            "method_used": "server.features",
            "ping": ping,
            "response": response,
            "resolved_ips": resolved_ips,
        })));
    }

    Err(error_response("All methods failed or server is unreachable"))
}




/// Fetch peers from an Electrum server
async fn fetch_peers(host: &str, port: u16) -> Result<Vec<Value>, String> {
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(addr).await.map_err(|e| {
        format!("Failed to connect to {}:{} - {}", host, port, e)
    })?;
    let tls_connector =
        TlsConnector::from(native_tls::TlsConnector::new().map_err(|e| e.to_string())?);
    let mut stream = tls_connector.connect(host, stream).await.map_err(|e| {
        format!("Failed to establish TLS connection to {}:{} - {}", host, port, e)
    })?;

    let request = json!({
        "id": 1,
        "method": "server.peers.subscribe",
        "params": []
    });
    let request_str = serde_json::to_string(&request).unwrap() + "\n";
    stream.write_all(request_str.as_bytes()).await.map_err(|e| {
        format!("Failed to send request to {}:{} - {}", host, port, e)
    })?;

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

    let response: Value = serde_json::from_str(&response_str).map_err(|e| {
        format!("Failed to parse JSON response from {}:{} - {}", host, port, e)
    })?;
    if let Some(peers) = response["result"].as_array() {
        Ok(peers.clone())
    } else {
        Err(format!("No peers found in response from {}:{}", host, port))
    }
}

/// Handler for `/electrum/peers`
pub async fn electrum_peers(Query(params): Query<PeerQueryParams>) -> Result<Json<serde_json::Value>, String> {
    let host = params.url;
    let port = params.port.unwrap_or(50002);

    let mut peers_map = serde_json::Map::new();

    // Try SSL first, fallback to TCP if SSL fails
    let (self_signed, _connection) = match try_connect(&host, port, true).await {
        Ok(result) => result,
        Err(_) => match try_connect(&host, 50001, false).await {
            Ok(result) => result,
            Err(e) => return Err(format!("Failed to connect to {}: {}", host, e)),
        },
    };

    // Fetch peers
    let peers = fetch_peers(&host, port).await?;
    for peer in peers {
        if let Some(peer_details) = peer.as_array() {
            let address = peer_details.get(0).and_then(|v| v.as_str()).unwrap_or("Unknown");
            let empty_vec = Vec::new();
            let features = peer_details
                .get(2)
                .and_then(|v| v.as_array())
                .unwrap_or(&empty_vec)
                .iter()
                .filter_map(|f| f.as_str())
                .collect::<Vec<&str>>();

            let version = features.iter().find_map(|f| f.strip_prefix('v')).unwrap_or("unknown");

            let peer_entry = serde_json::json!({
                "pruning": "-",
                "s": if features.iter().any(|&f| f.starts_with("s50002")) { Some("50002".to_string()) } else { None },
                "t": if features.iter().any(|&f| f.starts_with("t50001")) { Some("50001".to_string()) } else { None },
                "version": version,
                "self_signed": self_signed,
                "is_online": true,
            });

            peers_map.insert(address.to_string(), peer_entry);
        }
    }

    Ok(Json(serde_json::json!({ "peers": peers_map })))
}


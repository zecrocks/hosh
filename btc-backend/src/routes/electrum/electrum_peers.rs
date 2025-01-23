use crate::utils::{try_connect, error_response};
use axum::{extract::Query, response::Json};
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;
use std::pin::Pin;
use tokio_openssl::SslStream;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};



#[derive(Deserialize)]
pub struct PeerQueryParams {
    pub url: String,
    pub port: Option<u16>,
}

pub async fn fetch_peers(host: &str, port: u16) -> Result<Vec<Value>, String> {
    let (_self_signed, ssl_stream) = try_connect(host, port).await
        .map_err(|e| format!("Failed to connect to {}:{} - {}", host, port, e))?;

    let mut stream: Pin<Box<SslStream<TcpStream>>> = Box::pin(ssl_stream); // ✅ No more `Connection` enum

    let request = serde_json::json!({
        "id": 1,
        "method": "server.peers.subscribe",
        "params": []
    });

    let request_str = serde_json::to_string(&request).unwrap() + "\n";
    stream.write_all(request_str.as_bytes()).await.map_err(|e| {
        format!("Failed to send request to {}:{} - {}", host, port, e)
    })?;

    let mut buffer = Vec::new();
    let mut temp_buf = [0u8; 4096];

    loop {
        let n = stream.read(&mut temp_buf).await.map_err(|e| {
            format!("Failed to read response from {}:{} - {}", host, port, e)
        })?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&temp_buf[..n]);
        if buffer.ends_with(b"\n") {
            break;
        }
    }

    let response_str = String::from_utf8_lossy(&buffer);
    let response: Value = serde_json::from_str(&response_str)
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    let peers = response["result"]
        .as_array()
        .cloned()
        .ok_or_else(|| "Invalid response format".to_string())?;

    Ok(peers)
}

pub async fn electrum_peers(Query(params): Query<PeerQueryParams>) -> Result<Json<serde_json::Value>, axum::response::Response> {
    let host = &params.url;
    let port = params.port.unwrap_or(50002);

    let mut peers_map = serde_json::Map::new();

    let peers = fetch_peers(host, port).await.map_err(|e| {
        error_response(&format!("Failed to fetch peers: {}", e))
    })?;

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
            let pruning = features.iter().find_map(|f| f.strip_prefix("pruned")).unwrap_or("-");

            let peer_entry = json!({
                "pruning": pruning,
                "s": if features.iter().any(|&f| f.starts_with("s")) {
                    Some("50002".to_string())
                } else {
                    None
                },
                "t": if features.iter().any(|&f| f.starts_with("t")) {
                    Some("50001".to_string())
                } else {
                    None
                },
                "version": version
            });

            peers_map.insert(address.to_string(), peer_entry);
        }
    }

    Ok(Json(json!({ "peers": peers_map })))
}


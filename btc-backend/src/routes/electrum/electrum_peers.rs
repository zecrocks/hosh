use crate::utils::{try_connect, fetch_peers, error_response};
use axum::{extract::Query, response::Json};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct PeerQueryParams {
    pub url: String,
    pub port: Option<u16>,
}

pub async fn electrum_peers(Query(params): Query<PeerQueryParams>) -> Result<Json<serde_json::Value>, axum::response::Response> {
    let host = &params.url;
    let port = params.port.unwrap_or(50002);

    let mut peers_map = serde_json::Map::new();

    let (_self_signed, _connection) = try_connect(host, port, true).await.map_err(|e| {
        error_response(&format!("Failed to connect to {}:{} - {}", host, port, e))
    })?;


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



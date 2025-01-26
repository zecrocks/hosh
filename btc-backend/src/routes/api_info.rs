use axum::response::Json;
use serde::Serialize;

/// Struct to describe the API
#[derive(Serialize)]
pub struct ApiDescription {
    pub description: String,
    pub endpoints: Vec<EndpointInfo>,
}

/// Struct to describe an individual API endpoint
#[derive(Serialize)]
pub struct EndpointInfo {
    pub method: String,
    pub path: String,
    pub description: String,
    pub example_response: serde_json::Value,
}

/// Handler function for the `/` route
pub async fn api_info() -> Json<ApiDescription> {
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


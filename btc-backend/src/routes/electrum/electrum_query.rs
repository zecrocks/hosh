use crate::utils::{try_connect, error_response};
use axum::{extract::Query, response::Json};
use electrum_client::Client as ElectrumClient;
use electrum_client::ElectrumApi;
use serde::Deserialize;
use std::time::Instant;


#[derive(Deserialize)]
pub struct QueryParams {
    pub url: String,
    pub port: Option<u16>,
}

pub async fn electrum_query(Query(params): Query<QueryParams>) -> Result<Json<serde_json::Value>, axum::response::Response> {
    let host = &params.url;
    let port = params.port.unwrap_or(50002);

    let (self_signed, _connection) = match try_connect(host, port, true).await {
        Ok(result) => result,
        Err(e) => {
            eprintln!("SSL connection failed: {}", e);
            match try_connect(host, 50001, false).await {
                Ok(result) => result,
                Err(e) => return Err(error_response(&format!("Failed to connect to {}: {}", host, e))),
            }
        },
    };


    let client = ElectrumClient::new(&format!("ssl://{}:{}", host, port))
        .map_err(|e| error_response(&format!("Failed to create Electrum client: {}", e)))?;

    let resolved_ips = match tokio::net::lookup_host(format!("{}:{}", host, port)).await {
        Ok(addrs) => addrs.map(|addr| addr.ip().to_string()).collect::<Vec<String>>(),
        Err(_) => vec![],
    };

    // Attempt `blockchain.headers.subscribe`
    if let Ok(response) = client.raw_call("blockchain.headers.subscribe", Vec::new()) {
        let ping = Instant::now().elapsed().as_millis() as f64;
        return Ok(Json(serde_json::json!({
            "bits": response.get("bits"),
            "ping": ping,
            "resolved_ips": resolved_ips,
            "connection_type": if self_signed { "SSL (self-signed)" } else { "SSL" },
        })));
    }

    Err(error_response("All methods failed or server is unreachable"))
}

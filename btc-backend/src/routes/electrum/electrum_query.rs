use crate::utils::{try_connect, error_response};
use axum::{extract::Query, response::Json};
use electrum_client::{Client as ElectrumClient, ElectrumApi};
use serde::Deserialize;
use serde_json::json;
use std::time::{Duration, UNIX_EPOCH};
use chrono::{DateTime, Utc};
use bitcoin::blockdata::block::Header as BlockHeader;
use bitcoin::consensus::encode::deserialize;
use serde_json::Value;


#[derive(Deserialize)]
pub struct QueryParams {
    pub url: String,
    pub port: Option<u16>,
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
        "nonce": header.nonce as u32
    }))
}

pub async fn electrum_query(Query(params): Query<QueryParams>) -> Result<Json<serde_json::Value>, axum::response::Response> {
    let host = &params.url;
    let port = params.port.unwrap_or(50002);

    let (self_signed, _connection) = match try_connect(host, port, true).await {
        Ok(result) => result,
        Err(e) => {
            eprintln!("SSL connection failed: {}", e);
            return Err(error_response(&format!(
                "Failed to connect to {}:{} - {}", host, port, e
            )));
        }
    };

    let client = match ElectrumClient::new(&format!("ssl://{}:{}", host, port)) {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Failed to create Electrum client: {}", e);
            return Err(error_response(&format!(
                "Failed to create Electrum client: {}", e
            )));
        }
    };

    let resolved_ips = match tokio::net::lookup_host(format!("{}:{}", host, port)).await {
        Ok(addrs) => addrs.map(|addr| addr.ip().to_string()).collect::<Vec<String>>(),
        Err(_) => vec![],
    };

    let start_time = std::time::Instant::now();

    match client.raw_call("blockchain.headers.subscribe", Vec::new()) {
        Ok(response) => {
            let ping = start_time.elapsed().as_millis() as f64;

            let hex = response.get("hex").and_then(|v| v.as_str()).unwrap_or("");
            let parsed_header = match parse_block_header(hex) {
                Ok(header) => header,
                Err(e) => {
                    eprintln!("Failed to parse block header: {}", e);
                    json!({})
                }
            };

            // Extract timestamp from parsed header
            let timestamp = parsed_header.get("time").and_then(|v| v.as_u64()).unwrap_or(0);
            let timestamp_human = if timestamp > 0 {
                DateTime::<Utc>::from(UNIX_EPOCH + Duration::from_secs(timestamp)).to_rfc2822()
            } else {
                "Invalid timestamp".to_string()
            };

            let result = json!({
                "bits": parsed_header.get("bits").unwrap_or(&json!(null)),
                "connection_type": if self_signed { "SSL (self-signed)" } else { "SSL" },
                "error": "",
                "height": response.get("height").and_then(|v| v.as_u64()).unwrap_or(0),
                "host": host,
                "merkle_root": parsed_header.get("merkle_root").unwrap_or(&json!(null)),
                "method_used": "blockchain.headers.subscribe",
                "nonce": parsed_header.get("nonce").unwrap_or(&json!(null)),
                "ping": ping,
                "prev_block": parsed_header.get("prev_blockhash").unwrap_or(&json!(null)),
                "resolved_ips": resolved_ips,
                "self_signed": self_signed,
                "timestamp": timestamp,
                "timestamp_human": timestamp_human,
                "version": parsed_header.get("version").unwrap_or(&json!(null))
            });

            return Ok(Json(result));
        }
        Err(e) => {
            eprintln!("Error calling blockchain.headers.subscribe: {}", e);
            return Err(error_response(&format!(
                "Failed to query headers for {}:{} - {}", host, port, e
            )));
        }
    }
}





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
            println!("Raw response: {:?}", response); // Debug log

            let ping = start_time.elapsed().as_millis() as f64;

            let mut bits = response.get("bits").and_then(|v| v.as_u64()).unwrap_or(0);
            let mut nonce = response.get("nonce").and_then(|v| v.as_u64()).unwrap_or(0);
            let mut timestamp = response.get("timestamp").and_then(|v| v.as_u64()).unwrap_or(0);
            let mut version = response.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
            let mut merkle_root = response.get("merkle_root").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let mut prev_block_hash = response.get("prev_block_hash").and_then(|v| v.as_str()).unwrap_or("").to_string();

            // If the hex field is present, parse it and overwrite extracted fields
            if let Some(hex) = response.get("hex").and_then(|v| v.as_str()) {
                if let Ok(parsed_header) = parse_block_header(hex) {
                    bits = parsed_header.get("bits").and_then(|v| v.as_u64()).unwrap_or(bits);
                    nonce = parsed_header.get("nonce").and_then(|v| v.as_u64()).unwrap_or(nonce);
                    timestamp = parsed_header.get("time").and_then(|v| v.as_u64()).unwrap_or(timestamp);
                    version = parsed_header.get("version").and_then(|v| v.as_u64()).unwrap_or(version);
                    merkle_root = parsed_header.get("merkle_root").and_then(|v| v.as_str()).unwrap_or(&merkle_root).to_string();
                    prev_block_hash = parsed_header.get("prev_blockhash").and_then(|v| v.as_str()).unwrap_or(&prev_block_hash).to_string();
                }
            }

            let timestamp_human = if timestamp > 0 {
                DateTime::<Utc>::from(UNIX_EPOCH + Duration::from_secs(timestamp)).to_rfc2822()
            } else {
                "Invalid timestamp".to_string()
            };

            let result = json!({
                "bits": bits,
                "connection_type": if self_signed { "SSL (self-signed)" } else { "SSL" },
                "error": "",
                "height": response.get("block_height").and_then(|v| v.as_u64()).unwrap_or(0),
                "host": host,
                "merkle_root": merkle_root,
                "method_used": "blockchain.headers.subscribe",
                "nonce": nonce,
                "ping": ping,
                "prev_block_hash": prev_block_hash,
                "resolved_ips": resolved_ips,
                "self_signed": self_signed,
                "timestamp": timestamp,
                "timestamp_human": timestamp_human,
                "version": version
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







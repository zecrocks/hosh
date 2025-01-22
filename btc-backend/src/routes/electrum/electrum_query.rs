use crate::utils::{try_connect, error_response};
use axum::{extract::Query, response::Json};
use electrum_client::{Client as ElectrumClient, ElectrumApi};
use serde::Deserialize;
use serde_json::json;
use std::time::{Duration, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use bitcoin::blockdata::block::Header as BlockHeader;
use bitcoin::consensus::encode::deserialize;


#[derive(Deserialize)]
pub struct QueryParams {
    pub url: String,
    pub port: Option<u16>,
}


fn parse_block_header(header_hex: &str) -> Result<serde_json::Value, String> {
    let header_bytes = hex::decode(header_hex).map_err(|e| format!("Hex decode error: {}", e))?;
    let header: BlockHeader = deserialize(&header_bytes).map_err(|e| format!("Deserialize error: {}", e))?;

    // Reverse bytes for Python compatibility
    let prev_block_bytes: &[u8] = header.prev_blockhash.as_ref();
    let prev_block = prev_block_bytes.iter().rev().map(|b| format!("{:02x}", b)).collect::<String>();

    let merkle_root_bytes: &[u8] = header.merkle_root.as_ref();
    let merkle_root = merkle_root_bytes.iter().rev().map(|b| format!("{:02x}", b)).collect::<String>();

    Ok(serde_json::json!({
        "version": header.version,
        "prev_block": prev_block, // Correct naming
        "merkle_root": merkle_root,
        "timestamp": header.time,
        "timestamp_human": DateTime::<Utc>::from(UNIX_EPOCH + Duration::from_secs(header.time as u64)).to_rfc2822(),
        "bits": header.bits,
        "nonce": header.nonce as u32
    }))
}



fn process_raw_response(
    response: serde_json::Value,
    start_time: std::time::Instant,
    connection_type: &str,
    host: &str,
    resolved_ips: Vec<String>,
    self_signed: bool,
) -> Result<Json<serde_json::Value>, axum::response::Response> {
    let ping = start_time.elapsed().as_millis() as f64;

    // Extract fields
    let bits = response.get("bits").and_then(|v| v.as_u64()).unwrap_or(0);
    let nonce = response.get("nonce").and_then(|v| v.as_u64()).unwrap_or(0);
    let timestamp = response.get("timestamp").and_then(|v| v.as_u64()).unwrap_or(0);
    let version = response.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    let merkle_root = response.get("merkle_root").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let prev_block = response.get("prev_block").and_then(|v| v.as_str()).unwrap_or("").to_string(); // Correct field
    let height = response.get("height").and_then(|v| v.as_u64()).unwrap_or(0);

    // Convert timestamp
    let timestamp_human = if timestamp > 0 {
        DateTime::<Utc>::from(UNIX_EPOCH + Duration::from_secs(timestamp)).to_rfc2822()
    } else {
        "Invalid timestamp".to_string()
    };

    // Final result
    let result = json!({
        "bits": bits,
        "connection_type": connection_type,
        "error": "",
        "height": height, // Add height
        "host": host,
        "merkle_root": merkle_root,
        "method_used": "blockchain.headers.subscribe",
        "nonce": nonce,
        "ping": ping,
        "prev_block": prev_block, // Fix field name
        "resolved_ips": resolved_ips,
        "self_signed": self_signed,
        "timestamp": timestamp,
        "timestamp_human": timestamp_human,
        "version": version
    });

    Ok(Json(result))
}




pub async fn electrum_query(Query(params): Query<QueryParams>) -> Result<Json<serde_json::Value>, axum::response::Response> {
    let host = &params.url;
    let port = params.port.unwrap_or(50002);
    let is_onion_address = host.ends_with(".onion");

    let (self_signed, _connection) = try_connect(host, port, true)
        .await
        .map_err(|e| error_response(&format!("Failed to connect to {}:{} - {}", host, port, e)))?;

    // Determine connection type properly
    let connection_type = if is_onion_address {
        "Tor"
    } else if port == 50002 {
        "SSL"
    } else {
        "Plaintext"
    };

    // Handle potential errors when creating the Electrum client
    let client = if port == 50001 {
        ElectrumClient::new(&format!("tcp://{}:{}", host, port))
    } else {
        ElectrumClient::new(&format!("ssl://{}:{}", host, port))
    }
    .map_err(|e| {
        eprintln!("Error creating Electrum client: {}", e);
        error_response(&format!("Failed to create Electrum client for {}:{} - {}", host, port, e))
    })?;

    // Attempt to resolve the hostname, but don't fail the request if it doesn't resolve
    let resolved_ips = match tokio::net::lookup_host(format!("{}:{}", host, port)).await {
        Ok(addrs) => addrs.map(|addr| addr.ip().to_string()).collect::<Vec<String>>(),
        Err(e) => {
            eprintln!("Failed to resolve {}:{} - {}", host, port, e);
            vec![]
        }
    };

    let start_time = std::time::Instant::now();

    // Handle errors when calling Electrum API
    match client.raw_call("blockchain.headers.subscribe", Vec::new()) {
        Ok(response) => {
            println!("Electrum response: {:?}", response);

            let height = response.get("height").and_then(|v| v.as_u64()).unwrap_or(0);

            if let Some(hex_str) = response.get("hex").and_then(|v| v.as_str()) {
                match parse_block_header(hex_str) {
                    Ok(mut parsed_header) => {
                        // Add height field to parsed data
                        parsed_header["height"] = json!(height);
                        return process_raw_response(parsed_header, start_time, connection_type, host, resolved_ips, self_signed);
                    }
                    Err(e) => {
                        eprintln!("Failed to parse block header: {}", e);
                        return Err(error_response(&format!(
                            "Failed to parse block header for {}:{} - {}",
                            host, port, e
                        )));
                    }
                }
            }

            // If no hex field, just process response as usual
            process_raw_response(response, start_time, connection_type, host, resolved_ips, self_signed)
        },

        Err(e) => {
            eprintln!("Error calling blockchain.headers.subscribe: {}", e);
            Err(error_response(&format!("Failed to query headers for {}:{} - {}", host, port, e)))
        }
    }
}



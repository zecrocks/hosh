use crate::utils::{try_connect, error_response, send_electrum_request};
use axum::{extract::Query, response::Json};
use serde::Deserialize;
use serde_json::json;
use std::time::{Duration, UNIX_EPOCH};
use chrono::{DateTime, Utc};
use bitcoin::blockdata::block::Header as BlockHeader;
use bitcoin::consensus::encode::deserialize;
use crate::utils::ElectrumStream;



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
        "prev_block": prev_block,
        "merkle_root": merkle_root,
        "timestamp": header.time,
        "timestamp_human": DateTime::<Utc>::from(UNIX_EPOCH + Duration::from_secs(header.time as u64)).to_rfc2822(),
        "bits": header.bits,
        "nonce": header.nonce as u32
    }))
}


pub async fn electrum_query(Query(params): Query<QueryParams>) -> Result<Json<serde_json::Value>, axum::response::Response> {
    let host = &params.url;
    let port = params.port.unwrap_or(50002);
    
    println!("📥 Received query request for {}:{}", host, port);  // Add request received log
    
    let is_onion_address = host.ends_with(".onion");

    let (self_signed, mut stream) = try_connect(host, port).await
        .map_err(|e| {
            if e.contains("Failed to connect to .onion via Tor") {
                error_response(&format!("Failed to connect to {}:{} - {}", host, port, e), "tor_error")
            } else if e.contains("connection refused") || e.contains("Host unreachable") {
                error_response(&format!("Failed to connect to {}:{} - {}", host, port, e), "host_unreachable")
            } else {
                error_response(&format!("Failed to connect to {}:{} - {}", host, port, e), "connection_error")
            }
        })?;

    // Get server version first
    let version = match send_electrum_request(&mut stream, "server.version", vec![
        json!("btc-backend"), 
        json!(["1.4", "1.4.5"])
    ]).await {
        Ok(response) => {
            response.get("result")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.get(0))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown").to_string()
        },
        Err(e) => {
            eprintln!("Failed to get server version: {}", e);
            "unknown".to_string()
        }
    };

    let tls_version = match &stream {
        ElectrumStream::Ssl(ssl_stream) => ssl_stream.ssl().version_str().to_string(),
        ElectrumStream::Plain(_) => "None (plaintext)".to_string(),
    };

    println!(
        "🔍 Connected to {}:{} | TLS Version: {} | Self-signed: {:?}",
        host, port, tls_version, self_signed
    );

    let connection_type = if is_onion_address {
        "Tor"
    } else if matches!(stream, ElectrumStream::Ssl(_)) {
        "SSL"
    } else {
        "Plaintext"
    };

    let resolved_ips = if !is_onion_address {
        // Only attempt DNS lookup for non-.onion addresses
        match tokio::net::lookup_host(format!("{}:{}", host, port)).await {
            Ok(addrs) => addrs.map(|addr| addr.ip().to_string()).collect::<Vec<String>>(),
            Err(e) => {
                eprintln!("Failed to resolve {}:{} - {}", host, port, e);
                vec![]
            }
        }
    } else {
        // Skip DNS lookup for .onion addresses
        vec![]
    };

    let start_time = std::time::Instant::now(); // ✅ Start timing the request

    match send_electrum_request(&mut stream, "blockchain.headers.subscribe", vec![]).await {
        Ok(response) => {
            let ping = start_time.elapsed().as_millis() as f64;

            println!("Electrum response: {:?}", response);

            let height = response
                .get("result")
                .and_then(|r| r.get("height"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            if let Some(hex_str) = response.get("result").and_then(|r| r.get("hex")).and_then(|v| v.as_str()) {
                match parse_block_header(hex_str) {
                    Ok(parsed_header) => {
                        return Ok(Json(json!({
                            "error": "",
                            "method_used": "blockchain.headers.subscribe",
                            "host": host,
                            "height": height,
                            "ping": ping,
                            "tls_version": tls_version,
                            "self_signed": self_signed,
                            "connection_type": connection_type,
                            "resolved_ips": resolved_ips,
                            "server_version": version,
                            "bits": parsed_header["bits"],
                            "version": parsed_header["version"],
                            "nonce": parsed_header["nonce"],
                            "timestamp": parsed_header["timestamp"],
                            "timestamp_human": parsed_header["timestamp_human"]
                                .as_str().unwrap_or("").replace("+0000", "GMT"),
                            "merkle_root": parsed_header["merkle_root"],
                            "prev_block": parsed_header["prev_block"]
                        })));
                    }
                    Err(e) => {
                        eprintln!("Failed to parse block header: {}", e);
                        return Err(error_response(
                            &format!("Failed to parse block header for {}:{} - {}", host, port, e),
                            "parse_error"
                        ));
                    }
                }
            }

            Ok(Json(json!({
                "error": "",
                "method_used": "blockchain.headers.subscribe",
                "host": host,
                "height": height,
                "ping": ping,
                "tls_version": tls_version,
                "self_signed": self_signed,
                "connection_type": connection_type,
                "resolved_ips": resolved_ips,
                "server_version": version,
                "response": response
            })))
        },
        Err(e) => {
            eprintln!("Error calling blockchain.headers.subscribe: {}", e);
            Err(error_response(
                &format!("Failed to query headers for {}:{} - {}", host, port, e),
                "protocol_error"
            ))
        }
    }
}







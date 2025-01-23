use crate::utils::{try_connect, error_response};
use axum::{extract::Query, response::Json};
use serde::Deserialize;
use serde_json::json;
use std::time::{Duration, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_openssl::SslStream;
use tokio::net::TcpStream;

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
        "prev_block": prev_block,
        "merkle_root": merkle_root,
        "timestamp": header.time,
        "timestamp_human": DateTime::<Utc>::from(UNIX_EPOCH + Duration::from_secs(header.time as u64)).to_rfc2822(),
        "bits": header.bits,
        "nonce": header.nonce as u32
    }))
}

async fn send_electrum_request(
    ssl_stream: &mut SslStream<TcpStream>,
    method: &str,
    params: Vec<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let request = json!({
        "id": 1,
        "method": method,
        "params": params
    });

    let request_str = serde_json::to_string(&request).unwrap() + "\n";
    
    ssl_stream.write_all(request_str.as_bytes()).await.map_err(|e| {
        format!("❌ Failed to send request - {}", e)
    })?;

    let mut buffer = Vec::new();
    let mut temp_buf = [0u8; 4096];

    loop {
        let n = ssl_stream.read(&mut temp_buf).await.map_err(|e| {
            format!("❌ Failed to read response - {}", e)
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
    let response: serde_json::Value = serde_json::from_str(&response_str)
        .map_err(|e| format!("❌ Failed to parse JSON response - {}", e))?;

    Ok(response)
}

pub async fn electrum_query(Query(params): Query<QueryParams>) -> Result<Json<serde_json::Value>, axum::response::Response> {
    let host = &params.url;
    let port = params.port.unwrap_or(50002);
    let is_onion_address = host.ends_with(".onion");

    let (self_signed, mut ssl_stream) = try_connect(host, port).await
        .map_err(|e| error_response(&format!("Failed to connect to {}:{} - {}", host, port, e)))?;

    let tls_version = ssl_stream.ssl().version_str().to_string();

    println!(
        "🔍 Connected to {}:{} | TLS Version: {} | Self-signed: {}",
        host, port, tls_version, self_signed
    );

    let connection_type = if is_onion_address {
        "Tor"
    } else if port == 50002 {
        "SSL"
    } else {
        "Plaintext"
    };

    let resolved_ips = match tokio::net::lookup_host(format!("{}:{}", host, port)).await {
        Ok(addrs) => addrs.map(|addr| addr.ip().to_string()).collect::<Vec<String>>(),
        Err(e) => {
            eprintln!("Failed to resolve {}:{} - {}", host, port, e);
            vec![]
        }
    };

    let start_time = std::time::Instant::now(); // ✅ Start timing the request

    match send_electrum_request(&mut ssl_stream, "blockchain.headers.subscribe", vec![]).await {
        Ok(response) => {
            let ping = start_time.elapsed().as_millis() as f64; // ✅ Calculate ping (milliseconds)

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
                            "error": "", // ✅ Explicitly include an empty error string
                            "method_used": "blockchain.headers.subscribe", // ✅ Include the method used
                            "host": host, // ✅ Include host for completeness
                            "height": height,
                            "ping": ping, // ✅ Include measured ping time
                            "tls_version": tls_version,
                            "self_signed": self_signed,
                            "connection_type": connection_type,
                            "resolved_ips": resolved_ips,
                            "bits": parsed_header["bits"],
                            "version": parsed_header["version"],
                            "nonce": parsed_header["nonce"],
                            "timestamp": parsed_header["timestamp"],
                            "timestamp_human": parsed_header["timestamp_human"]
                                .as_str().unwrap_or("").replace("+0000", "GMT"), // ✅ Fix timestamp format
                            "merkle_root": parsed_header["merkle_root"],
                            "prev_block": parsed_header["prev_block"]
                        })));
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
                "response": response
            })))
        },
        Err(e) => {
            eprintln!("Error calling blockchain.headers.subscribe: {}", e);
            Err(error_response(&format!("Failed to query headers for {}:{} - {}", host, port, e)))
        }
    }
}







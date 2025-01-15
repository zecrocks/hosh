use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_native_tls::{TlsConnector, TlsStream};
use tokio_socks::{tcp::Socks5Stream}; // Ensure tokio_socks is included
use serde_json::Value;
use std::env;


pub enum Connection {
    Tcp(TcpStream),
    Tls(TlsStream<TcpStream>),
}

pub async fn try_connect(
    host: &str,
    port: u16,
    use_ssl: bool,
) -> Result<(bool, Connection), String> {
    if host.ends_with(".onion") {
        let tor_proxy_host = env::var("TOR_PROXY_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let tor_proxy_port = env::var("TOR_PROXY_PORT").unwrap_or_else(|_| "9050".to_string());

        println!("Connecting to .onion address via Tor proxy at {}:{}", tor_proxy_host, tor_proxy_port);

        // Establish SOCKS5 connection through the Tor proxy
        let stream = Socks5Stream::connect(
            (tor_proxy_host.as_str(), tor_proxy_port.parse::<u16>().unwrap()),
            (host, port),
        )
        .await
        .map_err(|e| format!("Failed to connect to .onion address via Tor: {}", e))?;

        if use_ssl {
            let tls_connector = TlsConnector::from(
                native_tls::TlsConnector::builder()
                    .danger_accept_invalid_certs(true) // Allow self-signed certificates
                    .build()
                    .map_err(|e| e.to_string())?,
            );

            let tls_stream = tls_connector
                .connect(host, stream.into_inner())
                .await
                .map_err(|e| format!("Failed to establish SSL connection: {}", e))?;

            return Ok((true, Connection::Tls(tls_stream)));
        } else {
            return Ok((false, Connection::Tcp(stream.into_inner())));
        }
    }

    // Standard connection logic for non-.onion addresses
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("Failed to connect to {}:{} - {}", host, port, e))?;

    if use_ssl {
        let tls_connector = TlsConnector::from(
            native_tls::TlsConnector::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .map_err(|e| e.to_string())?,
        );

        let tls_stream = tls_connector
            .connect(host, stream)
            .await
            .map_err(|e| format!("Failed to establish SSL connection: {}", e))?;

        return Ok((false, Connection::Tls(tls_stream)));
    } else {
        return Ok((false, Connection::Tcp(stream)));
    }
}




pub async fn fetch_peers(host: &str, port: u16) -> Result<Vec<Value>, String> {
    let addr = format!("{}:{}", host, port);

    let stream = TcpStream::connect(&addr).await.map_err(|e| {
        format!("Failed to connect to {}:{} - {}", host, port, e)
    })?;

    let tls_connector = TlsConnector::from(
        native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| e.to_string())?,
    );

    let mut stream = tls_connector.connect(host, stream).await.map_err(|e| {
        format!("Failed to establish TLS connection to {}:{} - {}", host, port, e)
    })?;

    let request = serde_json::json!({
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

pub fn error_response(message: &str) -> axum::response::Response {
    let error_body = serde_json::json!({ "error": message });
    axum::response::Response::builder()
        .status(400)
        .header("Content-Type", "application/json")
        .body(axum::body::boxed(axum::body::Full::from(error_body.to_string())))
        .unwrap()
}

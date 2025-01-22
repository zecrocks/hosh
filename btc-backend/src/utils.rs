use crate::tls::rustls_tls::{try_rustls_tls, RustlsConnection};
use crate::tls::native_tls::{try_native_tls, NativeTlsConnection};
use tokio_native_tls::TlsConnector as TokioNativeTlsConnector;

use native_tls::TlsConnector as NativeTlsConnector;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde_json::Value;

use tokio::net::TcpStream;

#[allow(dead_code)]
pub enum Connection {
    Tcp(TcpStream),
    Rustls(RustlsConnection),
    NativeTls(NativeTlsConnection),
}


pub async fn try_connect(
    host: &str,
    port: u16,
    use_ssl: bool,
) -> Result<(bool, Connection), String> {
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("Failed to connect to {}:{} - {}", host, port, e))?;

    if use_ssl {
        let std_stream = stream
            .into_std()
            .map_err(|e| format!("Failed to convert to StdTcpStream: {}", e))?;

        let cloned_std_stream = std_stream
            .try_clone()
            .map_err(|e| format!("Stream clone failed: {}", e))?;

        let cloned_stream = TcpStream::from_std(cloned_std_stream)
            .map_err(|e| format!("Failed to convert back to Tokio TcpStream: {}", e))?;

        match try_rustls_tls(host, cloned_stream).await {
            Ok((self_signed, conn)) => return Ok((self_signed, Connection::Rustls(conn))),
            Err(e) => println!("⚠️ Rustls failed: {}. Falling back to native-tls...", e),
        }

        let cloned_std_stream = std_stream
            .try_clone()
            .map_err(|e| format!("Stream clone failed: {}", e))?;

        let cloned_stream = TcpStream::from_std(cloned_std_stream)
            .map_err(|e| format!("Failed to convert back to Tokio TcpStream: {}", e))?;

        match try_native_tls(host, cloned_stream).await {
            Ok(conn) => return Ok((false, Connection::NativeTls(conn))), // Assume self-signed detection is Rustls-only
            Err(e) => return Err(format!("❌ Both Rustls and Native TLS failed: {}", e)),
        }
    }

    Ok((false, Connection::Tcp(stream)))
}




#[allow(dead_code)]
async fn send_initial_request<S: AsyncWriteExt + Unpin>(stream: &mut S) -> Result<(), String> {
    let request = serde_json::json!({"id": 1, "method": "server.version", "params": []});
    let request_str = serde_json::to_string(&request).unwrap() + "\n";
    stream.write_all(request_str.as_bytes()).await.map_err(|e| format!("Failed to send handshake request: {}", e))?;
    stream.flush().await.map_err(|e| format!("Failed to flush stream: {}", e))?;
    Ok(())
}

#[allow(dead_code)]
async fn read_server_response<S: AsyncReadExt + Unpin>(stream: &mut S) -> Result<(), String> {
    let mut buffer = Vec::new();
    let mut temp_buf = [0u8; 4096];
    loop {
        let n = stream.read(&mut temp_buf).await.map_err(|e| format!("Failed to read response: {}", e))?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&temp_buf[..n]);
        if buffer.ends_with(b"\n") {
            break;
        }
    }
    let response_str = String::from_utf8_lossy(&buffer);
    println!("Received response: {}", response_str);
    Ok(())
}

pub async fn fetch_peers(host: &str, port: u16) -> Result<Vec<Value>, String> {
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(&addr).await.map_err(|e| {
        format!("Failed to connect to {}:{} - {}", host, port, e)
    })?;

    let tls_connector = TokioNativeTlsConnector::from(
        NativeTlsConnector::builder()
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

    Ok(vec![])
}

pub fn error_response(message: &str) -> axum::response::Response {
    let error_body = serde_json::json!({ "error": message });
    axum::response::Response::builder()
        .status(400)
        .header("Content-Type", "application/json")
        .body(axum::body::boxed(axum::body::Full::from(error_body.to_string())))
        .unwrap()
}


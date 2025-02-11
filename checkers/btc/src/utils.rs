use std::pin::Pin;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use tokio::net::TcpStream;
use tokio_openssl::SslStream;
use tokio_socks::tcp::Socks5Stream; // Tor support
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use openssl::x509::X509StoreContextRef;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::env;
use serde_json::json;
use tokio::time::timeout;
use std::time::Duration;
use tracing::{debug, error, info, warn};


pub enum ElectrumStream {
    Plain(TcpStream),
    Ssl(SslStream<TcpStream>),
}

impl ElectrumStream {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        match self {
            ElectrumStream::Plain(tcp) => tcp.read(buf).await,
            ElectrumStream::Ssl(ssl) => ssl.read(buf).await,
        }
    }

    async fn write_all(&mut self, buf: &[u8]) -> Result<(), std::io::Error> {
        match self {
            ElectrumStream::Plain(tcp) => tcp.write_all(buf).await,
            ElectrumStream::Ssl(ssl) => ssl.write_all(buf).await,
        }
    }
}

pub async fn try_connect(
    host: &str,
    port: u16,
) -> Result<(Option<bool>, ElectrumStream), String> {
    info!("Attempting connection to {}:{}", host, port);

    let stream = if host.ends_with(".onion") {
        let tor_proxy_host = env::var("TOR_PROXY_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let tor_proxy_port = env::var("TOR_PROXY_PORT").unwrap_or_else(|_| "9050".to_string());

        info!("Using Tor proxy at {}:{} for .onion address", tor_proxy_host, tor_proxy_port);

        // Connect through Tor SOCKS5 proxy
        Socks5Stream::connect(
            (tor_proxy_host.as_str(), tor_proxy_port.parse::<u16>().unwrap()),
            (host, port),
        )
        .await
        .map_err(|e| {
            error!("Failed to connect to .onion via Tor: {}", e);
            format!("Failed to connect to .onion via Tor: {}", e)
        })?
        .into_inner()
    } else {
        let addr = format!("{}:{}", host, port);
        TcpStream::connect(&addr)
            .await
            .map_err(|e| {
                error!("Failed to connect to {}:{} - {}", host, port, e);
                format!("Failed to connect to {}:{} - {}", host, port, e)
            })?
    };

    info!("Successfully connected to {}:{}", host, port);

    // Plaintext connection (Port 50001)
    if port == 50001 {
        info!("Using plaintext connection (no SSL)");
        return Ok((None, ElectrumStream::Plain(stream)));
    }

    debug!("Establishing SSL connection...");

    let mut connector_builder = SslConnector::builder(SslMethod::tls())
        .map_err(|e| {
            error!("Failed to create OpenSSL connector: {:?}", e);
            format!("Failed to create OpenSSL connector: {:?}", e)
        })?;

    // Track self-signed certificates
    let self_signed_flag = Arc::new(AtomicBool::new(false));
    let flag_clone = Arc::clone(&self_signed_flag);

    connector_builder.set_verify_callback(SslVerifyMode::PEER, move |valid, _ctx: &mut X509StoreContextRef| {
        if !valid {
            warn!("Self-signed certificate detected");
            flag_clone.store(true, Ordering::Relaxed);
            return true; // Allow self-signed certs
        }
        valid
    });

    let connector = connector_builder.build();
    let config = connector.configure()
        .map_err(|e| {
            error!("Failed to configure OpenSSL: {:?}", e);
            format!("Failed to configure OpenSSL: {:?}", e)
        })?;

    let domain = host.to_string();
    let mut ssl = config.into_ssl(&domain)
        .map_err(|e| {
            error!("Failed to create OpenSSL SSL object: {:?}", e);
            format!("Failed to create OpenSSL SSL object: {:?}", e)
        })?;

    ssl.set_connect_state();

    let mut ssl_stream = SslStream::new(ssl, stream)
        .map_err(|e| {
            error!("Failed to create OpenSSL stream: {:?}", e);
            format!("Failed to create OpenSSL stream: {:?}", e)
        })?;

    let mut pinned_stream = Pin::new(&mut ssl_stream);

    match pinned_stream.as_mut().do_handshake().await {
        Ok(()) => {
            let self_signed = self_signed_flag.load(Ordering::Relaxed);
            let tls_version = ssl_stream.ssl().version_str();
            info!(
                "SSL handshake successful with {}:{} (TLS: {}, self_signed: {})",
                host, port, tls_version, self_signed
            );
            Ok((Some(self_signed), ElectrumStream::Ssl(ssl_stream)))
        }
        Err(e) => {
            error!("SSL handshake failed with {}:{} - {:?}", host, port, e);
            Err(format!("SSL handshake failed with {}:{} - {:?}", host, port, e))
        }
    }
}



pub async fn send_electrum_request(
    stream: &mut ElectrumStream,
    method: &str,
    params: Vec<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let request = json!({
        "id": 1,
        "method": method,
        "params": params
    });

    let request_str = serde_json::to_string(&request).unwrap() + "\n";
    debug!("Sending request: {}", request_str.trim());

    // Write with timeout
    timeout(Duration::from_secs(5), stream.write_all(request_str.as_bytes()))
        .await
        .map_err(|_| {
            error!("Write timeout occurred");
            "Write timeout".to_string()
        })?
        .map_err(|e| {
            error!("Write error: {}", e);
            format!("Write error: {}", e)
        })?;

    let mut buffer = Vec::new();
    let mut temp_buf = [0u8; 4096];

    // Read with timeout
    let read_future = async {
        loop {
            let n = match stream.read(&mut temp_buf).await {
                Ok(n) => n,
                Err(e) => {
                    error!("Read error: {}", e);
                    return Err(format!("Read error: {}", e));
                }
            };
            
            if n == 0 {
                if buffer.is_empty() {
                    error!("Empty response received");
                    return Err("Empty response".to_string());
                }
                break;
            }
            buffer.extend_from_slice(&temp_buf[..n]);
            if buffer.ends_with(b"\n") {
                break;
            }
        }
        Ok(buffer)
    };

    let buffer = timeout(Duration::from_secs(5), read_future)
        .await
        .map_err(|_| {
            error!("Read timeout occurred");
            "Read timeout".to_string()
        })?
        .map_err(|e| {
            error!("Read error: {}", e);
            format!("Read error: {}", e)
        })?;

    let response_str = String::from_utf8_lossy(&buffer);
    debug!("Received response: {}", response_str.trim());

    serde_json::from_str(&response_str)
        .map_err(|e| {
            error!("JSON parse error: {}", e);
            format!("JSON parse error: {}", e)
        })
}



pub fn error_response(message: &str, error_type: &str) -> axum::response::Response {
    error!(error_type = error_type, "API error: {}", message);
    let error_body = serde_json::json!({ 
        "error": message,
        "error_type": error_type 
    });
    axum::response::Response::builder()
        .status(400)
        .header("Content-Type", "application/json")
        .body(axum::body::boxed(axum::body::Full::from(error_body.to_string())))
        .unwrap()
}




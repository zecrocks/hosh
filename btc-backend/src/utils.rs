use std::pin::Pin;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use tokio::net::TcpStream;
use tokio_openssl::SslStream;
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use openssl::x509::X509StoreContextRef;


pub async fn try_connect(
    host: &str,
    port: u16,
) -> Result<(bool, SslStream<TcpStream>), String> {
    let addr = format!("{}:{}", host, port);
    println!("🔗 Attempting connection to {}:{}", host, port);

    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("❌ Failed to connect to {}:{} - {}", host, port, e))?;

    println!("✅ Successfully connected to {}:{}", host, port);

    println!("🔐 Establishing SSL connection...");

    let mut connector_builder = SslConnector::builder(SslMethod::tls())
        .map_err(|e| format!("❌ Failed to create OpenSSL connector: {:?}", e))?;

    // ✅ Fully disable certificate verification (allows self-signed certs)
    connector_builder.set_verify(SslVerifyMode::NONE);

    let self_signed_flag = Arc::new(AtomicBool::new(false));
    let flag_clone = Arc::clone(&self_signed_flag);

    // ✅ Allow self-signed certificates but track them
    connector_builder.set_verify_callback(SslVerifyMode::NONE, move |_valid, _ctx: &mut X509StoreContextRef| {
        flag_clone.store(true, Ordering::Relaxed);
        true
    });

    let connector = connector_builder.build();
    let config = connector.configure()
        .map_err(|e| format!("❌ Failed to configure OpenSSL: {:?}", e))?;

    let domain = host.to_string();
    let mut ssl = config.into_ssl(&domain)
        .map_err(|e| format!("❌ Failed to create OpenSSL SSL object: {:?}", e))?;

    ssl.set_connect_state();

    let mut ssl_stream = SslStream::new(ssl, stream)
        .map_err(|e| format!("❌ Failed to create OpenSSL stream: {:?}", e))?;

    let mut pinned_stream = Pin::new(&mut ssl_stream);

    match pinned_stream.as_mut().do_handshake().await {
        Ok(()) => {
            let self_signed = self_signed_flag.load(Ordering::Relaxed);
            let tls_version = ssl_stream.ssl().version_str();
            println!(
                "✅ SSL handshake successful with {}:{} (TLS: {}, self_signed: {})",
                host, port, tls_version, self_signed
            );
            Ok((self_signed, ssl_stream)) // ✅ Now returning the actual SSL stream
        }
        Err(e) => {
            let msg = format!("❌ SSL handshake failed with {}:{} - {:?}", host, port, e);
            println!("{}", msg);
            Err(msg)
        }
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




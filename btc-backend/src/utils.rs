use std::pin::Pin;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_openssl::SslStream;
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use openssl::x509::X509StoreContextRef;

/// Define a supertrait to avoid trait object restrictions
pub trait Stream: AsyncRead + AsyncWrite + Send + Sync + Unpin {}

impl<T: AsyncRead + AsyncWrite + Send + Sync + Unpin> Stream for T {}

pub enum Connection {
    Tcp(Pin<Box<TcpStream>>),
    OpenSsl(Pin<Box<SslStream<TcpStream>>>),
}

impl Connection {
    /// Returns a pinned boxed reference to the inner stream for read/write operations
    #[allow(dead_code)]
    pub fn get_mut(&mut self) -> Pin<&mut (dyn Stream)> {
        match self {
            Connection::Tcp(ref mut stream) => stream.as_mut(),
            Connection::OpenSsl(ref mut stream) => stream.as_mut(),
        }
    }
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
        let mut connector_builder = SslConnector::builder(SslMethod::tls())
            .map_err(|e| format!("Failed to create OpenSSL connector: {:?}", e))?;

        // ✅ Disable certificate verification (this fully allows self-signed certs)
        connector_builder.set_verify(SslVerifyMode::NONE);

        // ✅ Atomic flag to track if the certificate is self-signed
        let self_signed_flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&self_signed_flag);

        // ✅ Allow self-signed certificates but track them
        connector_builder.set_verify_callback(SslVerifyMode::PEER, move |valid, ctx: &mut X509StoreContextRef| {
            if !valid {
                let error_string = ctx.error().error_string();
                if error_string.contains("certificate verify failed") {
                    println!("⚠️ Warning: Self-signed certificate detected.");
                    flag_clone.store(true, Ordering::Relaxed); // ✅ Mark self-signed certs
                    return true; // ✅ Accept self-signed certificates
                }
            }
            valid
        });

        let connector = connector_builder.build();

        let config = connector.configure()
            .map_err(|e| format!("Failed to configure OpenSSL: {:?}", e))?;

        let domain = host.to_string();
        let mut ssl = config.into_ssl(&domain)
            .map_err(|e| format!("Failed to create OpenSSL SSL object: {:?}", e))?;

        ssl.set_connect_state(); // ✅ Explicitly set as a client connection

        let mut ssl_stream = SslStream::new(ssl, stream)
            .map_err(|e| format!("Failed to create OpenSSL stream: {:?}", e))?;

        let mut pinned_stream = Pin::new(&mut ssl_stream);

        match pinned_stream.as_mut().do_handshake().await {
            Ok(()) => {
                let self_signed = self_signed_flag.load(Ordering::Relaxed); // ✅ Read self-signed flag
                println!(
                    "✅ Successfully connected to {}:{} with SSL (self_signed: {})",
                    host, port, self_signed
                );
                Ok((self_signed, Connection::OpenSsl(Box::pin(ssl_stream))))
            }
            Err(e) => Err(format!("SSL handshake failed: {:?}", e)),
        }
    } else {
        println!("✅ Successfully connected to {}:{} without SSL", host, port);
        Ok((false, Connection::Tcp(Box::pin(stream))))
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




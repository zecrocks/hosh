
use tokio::net::TcpStream;
use native_tls::{TlsConnector as NativeTlsConnector, Protocol};
use tokio_native_tls::TlsStream as NativeTlsStream;
use tokio_native_tls::TlsConnector as TokioNativeTlsConnector;


pub struct NativeTlsConnection(pub NativeTlsStream<TcpStream>);

pub async fn try_native_tls(host: &str, stream: TcpStream) -> Result<NativeTlsConnection, String> {
    let connector = NativeTlsConnector::builder()
        .danger_accept_invalid_certs(true) // ✅ Accept self-signed certs
        .danger_accept_invalid_hostnames(true) // ✅ Ignore hostname mismatches
        .min_protocol_version(Some(Protocol::Tlsv10)) // ✅ Allow TLS 1.0+
        .max_protocol_version(Some(Protocol::Tlsv12)) // ✅ Force TLS 1.2
        .use_sni(true) // ✅ Explicitly enable SNI (Server Name Indication)
        .build()
        .map_err(|e| format!("Failed to create Native TLS connector: {:?}", e))?;
    
    let connector = TokioNativeTlsConnector::from(connector);

    match connector.connect(host, stream).await {
        Ok(tls_stream) => {
            println!("✅ Successfully connected with Native TLS!");

            // 🔍 Debugging: Print the certificate details
            match tls_stream.get_ref().peer_certificate() {
                Ok(Some(cert)) => println!("🔎 Peer Certificate: {:?}", cert.to_der()),
                Ok(None) => println!("⚠️ No peer certificate found."),
                Err(e) => println!("⚠️ Failed to retrieve peer certificate: {:?}", e),
            }

            Ok(NativeTlsConnection(tls_stream))
        }
        Err(e) => {
            println!("⚠️ Native TLS Handshake failed: {:?}", e);
            Err(format!("Native TLS connection failed: {:?}", e))
        }
    }
}

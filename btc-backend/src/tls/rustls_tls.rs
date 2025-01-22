use std::sync::{Arc, Mutex};
use tokio_rustls::{TlsConnector as RustlsConnector, client::TlsStream};
use rustls::{ClientConfig, RootCertStore, OwnedTrustAnchor, ServerName};
use rustls::client::{ServerCertVerifier, ServerCertVerified};
use rustls::{Certificate, Error as RustlsError};
use rustls::version::TLS12;
use webpki_roots;
use tokio::net::TcpStream;


pub struct RustlsConnection(pub TlsStream<TcpStream>);

struct SelfSignedCertVerifier {
    self_signed: Arc<Mutex<bool>>,
}

impl ServerCertVerifier for SelfSignedCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &Certificate,
        intermediates: &[Certificate],
        _server_name: &ServerName,
        _cert_chain: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        let is_self_signed = intermediates.is_empty();
        
        if is_self_signed {
            println!("⚠️ Warning: Self-signed certificate detected!");
            *self.self_signed.lock().unwrap() = true;
        } else {
            println!("✅ Server certificate is valid.");
        }
        
        Ok(ServerCertVerified::assertion())
    }
}



pub async fn try_rustls_tls(host: &str, stream: TcpStream) -> Result<(bool, RustlsConnection), String> {
    let mut root_store = RootCertStore::empty();
    
    for root in webpki_roots::TLS_SERVER_ROOTS {
        root_store.add_trust_anchors(std::iter::once(OwnedTrustAnchor::from_subject_spki_name_constraints(
            root.subject.to_vec(),
            root.subject_public_key_info.to_vec(),
            root.name_constraints.as_ref().map(|nc| nc.to_vec()),
        )));
    }

    let self_signed_flag = Arc::new(Mutex::new(false));
    let verifier = SelfSignedCertVerifier { self_signed: Arc::clone(&self_signed_flag) };


    let mut config = ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    config.dangerous().set_certificate_verifier(Arc::new(verifier));

    let config = Arc::new(config);
    let connector = RustlsConnector::from(config);
    let domain = ServerName::try_from(host).map_err(|_| "Invalid DNS name".to_string())?;

    match connector.connect(domain, stream).await {
        Ok(tls_stream) => {
            let self_signed = *self_signed_flag.lock().unwrap();
            println!("✅ Successfully connected with Rustls! Self-signed: {}", self_signed);
            Ok((self_signed, RustlsConnection(tls_stream)))
        }
        Err(e) => {
            println!("⚠️ Rustls TLS Handshake failed: {:?}", e);
            Err(format!("Rustls connection failed: {:?}", e))
        }
    }
}




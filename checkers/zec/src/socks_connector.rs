use tower::Service;
use tonic::transport::Uri;
use tokio_socks::tcp::Socks5Stream;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use hyper_util::rt::TokioIo;
use tracing::{debug, info, error};

/// A Tower Service that connects to gRPC servers through a SOCKS5 proxy.
/// 
/// This connector enables routing gRPC connections through a SOCKS proxy (e.g., Tor)
/// with remote DNS resolution, which is critical for connecting to .onion addresses
/// and maintaining anonymity.
/// 
/// ## Implementation Notes
/// 
/// - `poll_ready()` always returns `Poll::Ready(Ok(()))` because this is a stateless service
///   with no resource constraints. Each `call()` creates a fresh connection.
/// - Connection health checks, SOCKS proxy availability, and target reachability are all
///   verified in the `call()` future, not in `poll_ready()`.
/// - DNS resolution is performed remotely by the SOCKS proxy, not locally.
#[derive(Clone)]
pub struct SocksConnector {
    proxy_addr: String,
}

impl SocksConnector {
    /// Create a new SOCKS connector with the specified proxy address.
    /// 
    /// # Arguments
    /// * `proxy_addr` - SOCKS proxy address in the format "host:port" (e.g., "127.0.0.1:9050")
    pub fn new(proxy_addr: String) -> Self {
        info!("Creating SOCKS connector with proxy: {}", proxy_addr);
        Self { proxy_addr }
    }
}

impl Service<Uri> for SocksConnector {
    type Response = TokioIo<tokio::net::TcpStream>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Always ready - we're stateless with no resource constraints.
        // Each call() creates a fresh connection.
        // Connection health is checked in call(), not here.
        // This is for backpressure/capacity management, not health checking.
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let proxy_addr = self.proxy_addr.clone();
        
        Box::pin(async move {
            // Extract target host and port from URI
            let host = uri.host()
                .ok_or_else(|| Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Missing host in URI"
                )) as Box<dyn std::error::Error + Send + Sync>)?;
            
            let port = uri.port_u16().unwrap_or(443);
            let target = format!("{}:{}", host, port);
            
            info!("SOCKS: Connecting to {} via proxy {} (DNS will be resolved remotely)", target, proxy_addr);
            debug!("SOCKS: Using hostname '{}' for remote DNS resolution", host);
            
            // Connect through SOCKS proxy with remote DNS resolution.
            // This is where we actually check if:
            // - SOCKS proxy is reachable
            // - SOCKS handshake succeeds
            // - Target is reachable through proxy
            // 
            // Important: DNS resolution happens on the proxy side, not locally.
            // This is critical for .onion addresses and privacy.
            let socks_stream = Socks5Stream::connect(
                proxy_addr.as_str(),
                target.as_str()
            ).await
            .map_err(|e| {
                error!("SOCKS connection failed: {}", e);
                Box::new(std::io::Error::other(
                    format!("SOCKS proxy error: {}", e)
                )) as Box<dyn std::error::Error + Send + Sync>
            })?;
            
            info!("Successfully established SOCKS connection to {}", target);
            
            // Convert to TCP stream and wrap for hyper/tonic compatibility
            let tcp_stream = socks_stream.into_inner();
            Ok(TokioIo::new(tcp_stream))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_socks_connector_creation() {
        let connector = SocksConnector::new("127.0.0.1:9050".to_string());
        assert_eq!(connector.proxy_addr, "127.0.0.1:9050");
    }
    
    #[tokio::test]
    async fn test_poll_ready_always_ready() {
        use std::task::{Context, Poll};
        use futures_util::task::noop_waker;
        
        let mut connector = SocksConnector::new("127.0.0.1:9050".to_string());
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        
        // Should always be ready (stateless service)
        assert!(matches!(connector.poll_ready(&mut cx), Poll::Ready(Ok(()))));
        
        // Should remain ready on subsequent calls
        assert!(matches!(connector.poll_ready(&mut cx), Poll::Ready(Ok(()))));
    }
    
    #[test]
    fn test_uri_host_extraction() {
        // Test that we can extract host and port from various URI formats
        let test_cases = vec![
            ("https://zec.rocks:443", Some("zec.rocks"), Some(443)),
            ("https://example.com", Some("example.com"), None), // Will default to 443
            ("https://onion123.onion:443", Some("onion123.onion"), Some(443)),
        ];
        
        for (uri_str, expected_host, expected_port) in test_cases {
            let uri: Uri = uri_str.parse().unwrap();
            assert_eq!(uri.host(), expected_host);
            if let Some(port) = expected_port {
                assert_eq!(uri.port_u16(), Some(port));
            }
        }
    }
}


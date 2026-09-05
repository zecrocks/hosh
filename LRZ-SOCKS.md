# Routing librustzcash gRPC Through SOCKS5/Tor

This document explains how to connect to Zcash lightwalletd servers (including `.onion` hidden services) through a SOCKS5 proxy using librustzcash's gRPC client.

## Overview

The approach uses:
- **tokio-socks** - SOCKS5 client for async Rust
- **tower** - Service trait for custom connectors
- **tonic** - gRPC framework (used by librustzcash)
- **hyper-util** - IO compatibility layer

The key is implementing a `tower::Service` that wraps `tokio-socks`, which tonic's `Endpoint::connect_with_connector()` accepts.

## Dependencies

```toml
[dependencies]
tokio-socks = "0.5"
tonic = { version = "0.14", features = ["tls-webpki-roots"] }
tower = "0.5"
hyper-util = { version = "0.1", features = ["tokio"] }
tokio = { version = "1", features = ["full"] }
zcash_client_backend = { version = "0.21", features = ["lightwalletd-tonic-tls-webpki-roots"] }
```

## The SOCKS Connector

Implement `tower::Service<Uri>` to create connections through SOCKS5:

```rust
use tower::Service;
use tonic::transport::Uri;
use tokio_socks::tcp::Socks5Stream;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use hyper_util::rt::TokioIo;

#[derive(Clone)]
pub struct SocksConnector {
    proxy_addr: String,
}

impl SocksConnector {
    pub fn new(proxy_addr: String) -> Self {
        Self { proxy_addr }
    }
}

impl Service<Uri> for SocksConnector {
    type Response = TokioIo<tokio::net::TcpStream>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))  // Stateless - always ready
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let proxy_addr = self.proxy_addr.clone();

        Box::pin(async move {
            let host = uri.host()
                .ok_or_else(|| std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Missing host in URI"
                ))?;

            let port = uri.port_u16().unwrap_or(443);
            let target = format!("{}:{}", host, port);

            // Connect through SOCKS5 with 30s timeout
            // DNS resolution happens on the proxy (critical for .onion)
            let socks_stream = tokio::time::timeout(
                Duration::from_secs(30),
                Socks5Stream::connect(proxy_addr.as_str(), target.as_str())
            ).await
            .map_err(|_| std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "SOCKS connection timed out"
            ))??;

            // Wrap for tonic/hyper compatibility
            Ok(TokioIo::new(socks_stream.into_inner()))
        })
    }
}
```

## Using with librustzcash

Connect to a lightwalletd server through Tor:

```rust
use tonic::transport::{ClientTlsConfig, Endpoint, Uri};
use zcash_client_backend::proto::service::{
    compact_tx_streamer_client::CompactTxStreamerClient,
    Empty,
};
use std::time::Duration;

async fn get_lightwalletd_info(
    host: &str,
    port: u16,
    socks_proxy: &str,  // e.g., "127.0.0.1:9050"
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let connector = SocksConnector::new(socks_proxy.to_string());

    let is_onion = host.ends_with(".onion");

    // .onion services don't need TLS - Tor provides encryption
    let uri = if is_onion {
        format!("http://{}:{}", host, port).parse::<Uri>()?
    } else {
        format!("https://{}:{}", host, port).parse::<Uri>()?
    };

    let mut endpoint = Endpoint::from(uri.clone())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20));

    // Only add TLS for non-.onion addresses
    if !is_onion {
        endpoint = endpoint.tls_config(ClientTlsConfig::new().with_webpki_roots())?;
    }

    // Connect using our SOCKS connector
    let channel = endpoint.connect_with_connector(connector).await?;

    let mut client = CompactTxStreamerClient::with_origin(channel, uri);

    let response = client.get_lightd_info(Empty {}).await?;
    let info = response.into_inner();

    Ok(info.block_height)
}
```

## Key Points

### Remote DNS Resolution

SOCKS5 resolves DNS on the proxy side, not locally. This is essential for:
- `.onion` addresses (only resolvable by Tor)
- Privacy (no DNS leaks)

### .onion TLS Handling

Tor hidden services typically don't use TLS because Tor already encrypts end-to-end. Use `http://` scheme for `.onion`:

```rust
let uri = if host.ends_with(".onion") {
    format!("http://{}:{}", host, port)  // No TLS needed
} else {
    format!("https://{}:{}", host, port) // Regular TLS
};
```

### Timeouts

Tor circuit building can be slow. Use generous timeouts:

| Stage | Recommended |
|-------|-------------|
| SOCKS connect | 30s |
| gRPC dial | 10s |
| gRPC call | 20s |

### Error Handling

Common failure modes:
- SOCKS proxy unreachable
- Tor circuit timeout
- Target server offline
- TLS handshake failure (regular servers)

## Docker Setup

Run a Tor SOCKS proxy:

```yaml
services:
  tor:
    image: osminogin/tor-simple
    ports:
      - "9050:9050"
    healthcheck:
      test: ["CMD-SHELL", "curl --fail --socks5-hostname localhost:9050 https://check.torproject.org/"]
      interval: 30s

  your-app:
    environment:
      - SOCKS_PROXY=tor:9050
    depends_on:
      tor:
        condition: service_healthy
```

## Full Working Example

```rust
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let proxy = env::var("SOCKS_PROXY").unwrap_or_else(|_| "127.0.0.1:9050".into());

    // Test with a public lightwalletd server
    let height = get_lightwalletd_info("mainnet.lightwalletd.com", 9067, &proxy).await?;
    println!("Block height: {}", height);

    // Or a .onion server
    // let height = get_lightwalletd_info("example.onion", 9067, &proxy).await?;

    Ok(())
}
```

## References

- Source: `crates/hosh-checker-zec/src/socks_connector.rs`
- Usage: `crates/hosh-checker-zec/src/lib.rs`

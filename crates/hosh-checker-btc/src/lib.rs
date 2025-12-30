//! Hosh Checker BTC - Bitcoin/Electrum server health checker.
//!
//! This crate provides functionality to check the health of Bitcoin Electrum servers.
//! It can run in two modes:
//! - Worker mode: Polls for jobs and checks servers
//! - Server mode: Provides an API for querying Electrum servers

use axum::{routing::get, Router};
use std::net::SocketAddr;
use tracing::{error, info};

pub mod routes;
pub mod utils;
pub mod worker;

use routes::{
    api_info::api_info,
    electrum::{electrum_peers, electrum_query, electrum_servers},
    health::health_check,
};

/// Run the BTC checker in worker mode.
///
/// This is the primary mode for production use. The worker polls the web API
/// for jobs and checks Bitcoin/Electrum servers.
pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_worker().await
}

/// Run the BTC checker in worker mode.
pub async fn run_worker() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Starting BTC checker in worker mode...");
    match worker::Worker::new().await {
        Ok(worker) => {
            if let Err(e) = worker.run().await {
                error!("Worker error: {}", e);
                return Err(e);
            }
        }
        Err(e) => {
            error!("Failed to create worker: {}", e);
            return Err(e);
        }
    }
    Ok(())
}

/// Run the BTC checker in API server mode.
///
/// This mode provides an HTTP API for querying Electrum servers directly.
/// Useful for debugging and development.
pub async fn run_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Starting BTC checker in API server mode...");

    let app = Router::new()
        .route("/", get(api_info))
        .route("/healthz", get(health_check))
        .route("/electrum/servers", get(electrum_servers))
        .route("/electrum/query", get(electrum_query))
        .route("/electrum/peers", get(electrum_peers));

    let addr = SocketAddr::from(([0, 0, 0, 0], 5000));
    info!("Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    if let Err(e) = axum::serve(listener, app).await {
        error!("Server error: {}", e);
        return Err(Box::new(e));
    }

    Ok(())
}

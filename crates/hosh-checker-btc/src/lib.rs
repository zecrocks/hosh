//! Hosh Checker BTC - Bitcoin/Electrum server health checker.
//!
//! This crate provides functionality to check the health of Bitcoin Electrum servers.
//! It runs in worker mode, polling the web API for jobs and checking servers.

use tracing::{error, info};

pub mod routes;
pub mod utils;
pub mod worker;

/// Run the BTC checker in worker mode.
///
/// This is the primary mode for production use. The worker polls the web API
/// for jobs and checks Bitcoin/Electrum servers.
pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

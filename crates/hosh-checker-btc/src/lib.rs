//! Hosh Checker BTC - Bitcoin/Electrum server health checker.
//!
//! This crate provides functionality to check the health of Bitcoin Electrum servers.
//! It runs in worker mode, polling the web API for jobs and checking servers.

use tracing::{error, info};

pub mod routes;
pub mod utils;
pub mod worker;

/// Run the BTC checker in worker mode with default location.
///
/// This is the primary mode for production use. The worker polls the web API
/// for jobs and checks Bitcoin/Electrum servers.
pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_with_location("dfw").await
}

/// Run the BTC checker in worker mode with a specified location.
///
/// The location identifier is included in all check results to enable
/// multi-region monitoring.
pub async fn run_with_location(
    location: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!(
        "Starting BTC checker in worker mode (location: {})...",
        location
    );
    match worker::Worker::new_with_location(location).await {
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

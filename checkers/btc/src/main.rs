use axum::{
    routing::get,
    Router,
};
use std::net::SocketAddr;
use tracing::{info, error, Level};
use tracing_subscriber;

mod routes;
mod utils;
mod worker;

use routes::{
    api_info::api_info,
    health::health_check,
    electrum::{electrum_servers, electrum_query, electrum_peers},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    info!("🚀 Starting BTC checker...");

    // Check if we should run in worker mode
    let is_worker = std::env::var("RUN_MODE")
        .map(|v| v == "worker")
        .unwrap_or(false);

    if is_worker {
        info!("Starting in worker mode...");
        match worker::Worker::new().await {
            Ok(worker) => {
                if let Err(e) = worker.run().await {
                    error!("Worker error: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to create worker: {}", e);
            }
        }
    } else {
        info!("Starting in API server mode...");
        // Define the routes for the application
        let app = Router::new()
            .route("/", get(api_info))
            .route("/healthz", get(health_check))
            .route("/electrum/servers", get(electrum_servers))
            .route("/electrum/query", get(electrum_query))
            .route("/electrum/peers", get(electrum_peers));

        // Define the address to bind the server to
        let addr = SocketAddr::from(([0, 0, 0, 0], 5000));
        info!("Server running on http://{}", addr);

        // Start the server
        if let Err(e) = axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
        {
            error!("Server error: {}", e);
        }
    }

    Ok(())
}

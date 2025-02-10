use axum::{
    routing::get,
    Router,
};
use std::net::SocketAddr;

mod routes;
mod utils;
mod worker;

use routes::{
    api_info::api_info,
    health::health_check,
    electrum::{electrum_servers, electrum_query, electrum_peers},
};

#[tokio::main]
async fn main() {
    // Check if we should run in worker mode
    let is_worker = std::env::var("RUN_MODE")
        .map(|v| v == "worker")
        .unwrap_or(false);

    if is_worker {
        println!("Starting in worker mode...");
        match worker::Worker::new().await {
            Ok(worker) => {
                if let Err(e) = worker.run().await {
                    eprintln!("Worker error: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Failed to create worker: {}", e);
            }
        }
    } else {
        println!("Starting in API server mode...");
        // Define the routes for the application
        let app = Router::new()
            .route("/", get(api_info))
            .route("/healthz", get(health_check))
            .route("/electrum/servers", get(electrum_servers))
            .route("/electrum/query", get(electrum_query))
            .route("/electrum/peers", get(electrum_peers));

        // Define the address to bind the server to
        let addr = SocketAddr::from(([0, 0, 0, 0], 5000));
        println!("Server running on http://{}", addr);

        // Start the server
        if let Err(e) = axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
        {
            eprintln!("Server error: {}", e);
        }
    }
}

use axum::{
    routing::get,
    Router,
};
use std::net::SocketAddr;
use tracing_subscriber;

mod routes;
use routes::{
    api_info::api_info,
    health::health_check,
    electrum::{electrum_servers, electrum_query, electrum_peers},
};

#[tokio::main]
async fn main() {
    // Set up a tracing subscriber for logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG) // Set the maximum log level
        .with_target(true) // Include target in the logs
        .init();

    // Define the routes for the application
    let app = Router::new()
        .route("/", get(api_info))
        .route("/healthz", get(health_check))
        .route("/electrum/servers", get(electrum_servers))
        .route("/electrum/query", get(electrum_query))
        .route("/electrum/peers", get(electrum_peers));

    // Define the address to bind the server to
    let addr = SocketAddr::from(([0, 0, 0, 0], 5000));
    tracing::info!("Server running on http://{}", addr);

    // Start the server
    if let Err(e) = axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
    {
        tracing::error!("Server error: {}", e);
    }
}


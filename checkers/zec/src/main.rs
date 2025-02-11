use std::{env, error::Error};
use chrono::{DateTime, Utc};
use redis::Commands;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use zingolib;
use futures_util::StreamExt;
use tokio::task;
use tracing::{info, error};

#[derive(Debug, Deserialize)]
struct CheckRequest {
    host: String,
    port: u16,
    check_id: Option<String>,
    user_submitted: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CheckResult {
    host: String,
    port: u16,
    height: u64,
    status: String,
    error: Option<String>,
    #[serde(rename = "LastUpdated")]
    last_updated: DateTime<Utc>,
    ping: f64,
    check_id: Option<String>,
    user_submitted: Option<bool>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let nats_prefix = env::var("NATS_PREFIX").unwrap_or_else(|_| "hosh.".into());
    let nats_url = format!(
        "nats://{}:{}",
        env::var("NATS_HOST").unwrap_or_else(|_| "nats".into()),
        env::var("NATS_PORT").unwrap_or_else(|_| "4222".into())
    );

    let redis_url = format!(
        "redis://{}:{}",
        env::var("REDIS_HOST").unwrap_or_else(|_| "redis".into()),
        env::var("REDIS_PORT").unwrap_or_else(|_| "6379".into())
    );

    let mut redis_conn = redis::Client::open(redis_url.as_str())?.get_connection()?;
    info!("Connected to Redis at {}", redis_url);
    
    let nc = async_nats::connect(&nats_url).await?;
    info!("Connected to NATS at {}", nats_url);
    
    let mut sub = nc.subscribe(format!("{}check.zec", nats_prefix)).await?;
    info!("Subscribed to {}check.zec", nats_prefix);

    while let Some(msg) = sub.next().await {
        let check_request: CheckRequest = match serde_json::from_slice(&msg.payload) {
            Ok(req) => req,
            Err(e) => {
                error!("Failed to parse check request: {e}");
                continue;
            }
        };

        let uri = match format!("https://{}:{}", check_request.host, check_request.port).parse() {
            Ok(u) => u,
            Err(e) => {
                error!("Invalid URI: {e}");
                continue;
            }
        };

        let start_time = Instant::now();
        let (height, error) = match task::spawn_blocking(move || {
            zingolib::get_latest_block_height(uri)
        }).await? {
            Ok(h) => (h, None),
            Err(e) => (0, Some(e.to_string())),
        };

        let latency = start_time.elapsed().as_secs_f64() * 1000.0;
        let ping = (latency * 100.0).round() / 100.0;
        let status = if error.is_none() { "success" } else { "error" };

        match &error {
            Some(err) => info!(
                "Server {}:{} - Error checking block height, Latency: {:.2}ms, Error: {}",
                check_request.host, check_request.port, ping, err
            ),
            None => info!(
                "Server {}:{} - Block height: {}, Latency: {:.2}ms",
                check_request.host, check_request.port, height, ping
            ),
        }

        let result = CheckResult {
            host: check_request.host.clone(),
            port: check_request.port,
            height,
            status: status.into(),
            error,
            last_updated: Utc::now(),
            ping,
            check_id: check_request.check_id,
            user_submitted: check_request.user_submitted,
        };

        if let Ok(result_json) = serde_json::to_string(&result) {
            let redis_key = format!("zec:{}", check_request.host);
            if let Err(e) = redis_conn.set::<_, _, ()>(&redis_key, &result_json) {
                error!("Redis save failed: {e}");
            }
        }
    }

    Ok(())
}

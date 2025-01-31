use std::{env, error::Error, time::Instant};
use chrono::{DateTime, Utc};
use redis::Commands;
use serde::{Deserialize, Serialize};
use zingolib;

#[derive(Debug, Deserialize)]
struct CheckRequest {
    host: String,
    port: u16,
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
}

fn main() -> Result<(), Box<dyn Error>> {
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
    println!("Connected to Redis at {}", redis_url);
    
    let nc = nats::connect(&nats_url)?;
    println!("Connected to NATS at {}", nats_url);
    
    let sub = nc.subscribe(&format!("{}check.zec", nats_prefix))?;
    println!("Subscribed to {}check.zec", nats_prefix);
    nc.flush()?;

    while let Some(msg) = sub.next() {
        let check_request: CheckRequest = match serde_json::from_slice(&msg.data) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Failed to parse check request: {e}");
                continue;
            }
        };

        let uri = match format!("https://{}:{}", check_request.host, check_request.port).parse() {
            Ok(u) => u,
            Err(e) => {
                eprintln!("Invalid URI: {e}");
                continue;
            }
        };

        let start_time = Instant::now();
        let (height, error) = match zingolib::get_latest_block_height(uri) {
            Ok(h) => (h, None),
            Err(e) => (0, Some(e.to_string())),
        };

        let latency = start_time.elapsed().as_secs_f64() * 1000.0;
        let ping = (latency * 100.0).round() / 100.0;
        let status = if error.is_none() { "success" } else { "error" };

        match &error {
            Some(err) => println!(
                "Server {}:{} - Error checking block height, Latency: {:.2}ms, Error: {}",
                check_request.host, check_request.port, ping, err
            ),
            None => println!(
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
        };

        if let Ok(result_json) = serde_json::to_string(&result) {
            let redis_key = format!("zec:{}", check_request.host);
            if let Err(e) = redis_conn.set::<_, _, ()>(&redis_key, &result_json) {
                eprintln!("Redis save failed: {e}");
            }
        }
    }

    Ok(())
}

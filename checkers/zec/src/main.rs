use std::error::Error;
use http::Uri;
use rustls::crypto::ring::default_provider;
use zingolib;
use serde::{Deserialize, Serialize};
use std::env;
use chrono::{DateTime, Utc};
use redis::Commands;
use std::time::Instant;

#[derive(Debug, Deserialize)]
struct CheckRequest {
    host: String,
    port: u16,
}

#[derive(Debug, Serialize)]
struct CheckResult {
    host: String,
    port: u16,
    block_height: u64,
    status: String,
    error: Option<String>,
    #[serde(rename = "LastUpdated")]
    last_updated: DateTime<Utc>,
    ping: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Install crypto provider
    default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let nats_prefix = env::var("NATS_PREFIX").unwrap_or_else(|_| "hosh.".to_string());
    let nats_host = env::var("NATS_HOST").unwrap_or_else(|_| "nats".to_string());
    let nats_port = env::var("NATS_PORT").unwrap_or_else(|_| "4222".to_string());
    let nats_url = format!("nats://{}:{}", nats_host, nats_port);

    let redis_host = env::var("REDIS_HOST").unwrap_or_else(|_| "redis".to_string());
    let redis_port = env::var("REDIS_PORT").unwrap_or_else(|_| "6379".to_string());
    let redis_url = format!("redis://{}:{}", redis_host, redis_port);

    let redis_client = redis::Client::open(redis_url.as_str())?;
    let mut redis_conn = redis_client.get_connection()?;
    println!("Connected to Redis at {}", redis_url);

    let nc = nats::connect(&nats_url)?;
    println!("Connected to NATS at {}", nats_url);

    let sub = nc.subscribe(&format!("{}check.zec", nats_prefix))?;
    println!("Subscribed to {}check.zec", nats_prefix);

    for msg in sub.messages() {
        // Parse the check request
        let check_request: CheckRequest = match serde_json::from_slice(&msg.data) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Failed to parse check request: {}", e);
                continue;
            }
        };

        println!("Checking ZEC server {}:{}", check_request.host, check_request.port);

        let uri: Uri = format!("https://{}:{}", check_request.host, check_request.port)
            .parse()
            .expect("Failed to parse URI");

        let start_time = Instant::now();
        let result = match zingolib::get_latest_block_height(uri) {
            Ok(height) => {
                let elapsed = start_time.elapsed();
                let latency = elapsed.as_secs_f64() * 1000.0;
                let rounded_latency = (latency * 100.0).round() / 100.0;
                println!(
                    "Server {}:{} - Block height: {}, Latency: {:.2}ms",
                    check_request.host,
                    check_request.port,
                    height,
                    rounded_latency
                );
                CheckResult {
                    host: check_request.host.clone(),
                    port: check_request.port,
                    block_height: height,
                    status: "success".to_owned(),
                    error: None,
                    last_updated: Utc::now(),
                    ping: rounded_latency,
                }
            },
            Err(e) => {
                let elapsed = start_time.elapsed();
                let latency = elapsed.as_secs_f64() * 1000.0;
                let rounded_latency = (latency * 100.0).round() / 100.0;
                eprintln!(
                    "Server {}:{} - Error checking block height, Latency: {:.2}ms, Error: {}",
                    check_request.host,
                    check_request.port,
                    rounded_latency,
                    e
                );
                CheckResult {
                    host: check_request.host.clone(),
                    port: check_request.port,
                    block_height: 0,
                    status: "error".to_owned(),
                    error: Some(e.to_string()),
                    last_updated: Utc::now(),
                    ping: rounded_latency,
                }
            },
        };

        if let Ok(result_json) = serde_json::to_string(&result) {
            let redis_key = format!("zec:{}", check_request.host);
            if let Err(e) = redis_conn.set::<_, _, ()>(&redis_key, &result_json) {
                eprintln!("Failed to save to Redis: {}", e);
            }
        }

        // Publish the result to NATS for a future persistence database
        // if let Ok(result_json) = serde_json::to_string(&result) {
        //     if let Err(e) = nc.publish("hosh.result.zec", result_json) {
        //         eprintln!("Failed to publish result: {}", e);
        //     }
        // }
    }

    Ok(())
}

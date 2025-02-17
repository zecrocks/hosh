use redis::Commands;
use std::error::Error;
use std::fmt;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::time::Instant;

mod blockchair;
mod blockchain;
mod blockstream;
mod mempool;
mod zecrocks;
mod zcashexplorer;

// Keep this import since we'll use it as our canonical BlockchainInfo
use blockchain::BlockchainInfo;

#[derive(Debug)]
enum CheckerError {
    Redis(redis::RedisError),
    Reqwest(reqwest::Error),
    Var(std::env::VarError),
    Other(String),
}

impl fmt::Display for CheckerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CheckerError::Redis(e) => write!(f, "Redis error: {}", e),
            CheckerError::Reqwest(e) => write!(f, "Request error: {}", e),
            CheckerError::Var(e) => write!(f, "Environment variable error: {}", e),
            CheckerError::Other(e) => write!(f, "Error: {}", e),
        }
    }
}

impl Error for CheckerError {}

impl From<redis::RedisError> for CheckerError {
    fn from(err: redis::RedisError) -> CheckerError {
        CheckerError::Redis(err)
    }
}

impl From<reqwest::Error> for CheckerError {
    fn from(err: reqwest::Error) -> CheckerError {
        CheckerError::Reqwest(err)
    }
}

impl From<std::env::VarError> for CheckerError {
    fn from(err: std::env::VarError) -> CheckerError {
        CheckerError::Var(err)
    }
}

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
    let nats_prefix = std::env::var("NATS_PREFIX").unwrap_or_else(|_| "hosh.".into());
    let nats_url = format!(
        "nats://{}:{}",
        std::env::var("NATS_HOST").unwrap_or_else(|_| "nats".into()),
        std::env::var("NATS_PORT").unwrap_or_else(|_| "4222".into())
    );

    let redis_url = format!(
        "redis://{}:{}",
        std::env::var("REDIS_HOST").unwrap_or_else(|_| "redis".into()),
        std::env::var("REDIS_PORT").unwrap_or_else(|_| "6379".into())
    );
    
    println!("🔌 Connecting to Redis at {}", redis_url);
    let client = redis::Client::open(redis_url.as_str())?;
    let mut con = client.get_connection()?;

    let nc = async_nats::connect(&nats_url).await?;
    println!("Connected to NATS at {}", nats_url);
    
    let mut sub = nc.subscribe(format!("{}check.http", nats_prefix)).await?;
    println!("Subscribed to {}check.http", nats_prefix);

    while let Some(msg) = sub.next().await {
        let _check_request: CheckRequest = match serde_json::from_slice(&msg.payload) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Failed to parse check request: {e}");
                continue;
            }
        };

        // Fetch data from all sources concurrently
        let (blockstream_result, zecrocks_result, blockchair_result, blockchain_result, zcashexplorer_result) = tokio::join!(
            blockstream::get_blockchain_info(),
            zecrocks::get_blockchain_info(),
            blockchair::get_blockchain_info(),
            blockchain::get_blockchain_info(),
            zcashexplorer::get_blockchain_info()
        );

        // Add blockstream data
        if let Ok(data) = blockstream_result {
            for (chain, info) in data {
                if let Some(height) = info.height {
                    println!("{} height: {} (blockstream)", info.name, height);
                    let _: () = con.set(
                        format!("http:blockstream.{}", chain),
                        height
                    )?;
                }
            }
        }

        // Add zecrocks data
        if let Ok(data) = zecrocks_result {
            for (chain, info) in data {
                if let Some(height) = info.height {
                    println!("{} height: {} (zecrocks)", info.name, height);
                    let _: () = con.set(
                        format!("http:zecrocks.{}", chain),
                        height
                    )?;
                }
            }
        }

        // Add blockchair data
        if let Ok(data) = blockchair_result {
            for (chain, info) in data {
                if let Some(height) = info.height {
                    println!("{} height: {} (blockchair)", info.name, height);
                    let _: () = con.set(
                        format!("http:blockchair.{}", chain),
                        height
                    )?;
                }
            }
        }

        // Add blockchain.com data
        if let Ok(data) = blockchain_result {
            for (chain, info) in data {
                if let Some(height) = info.height {
                    println!("{} height: {} (blockchain)", info.name, height);
                    let _: () = con.set(
                        format!("http:blockchain.{}", chain),
                        height
                    )?;
                }
            }
        }

        // Add zcashexplorer data
        if let Ok(data) = zcashexplorer_result {
            for (chain, info) in data {
                if let Some(height) = info.height {
                    println!("{} height: {} (zcashexplorer)", info.name, height);
                    let _: () = con.set(
                        format!("http:zcashexplorer.{}", chain),
                        height
                    )?;
                }
            }
        }
    }

    Ok(())
}
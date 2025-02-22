use redis::Commands;
use std::error::Error;
use std::fmt;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

mod blockchair;
mod blockchaindotcom;
mod blockstream;
mod mempool;
mod zecrocks;
mod zcashexplorer;
mod types;

use crate::types::BlockchainInfo;

#[derive(Debug)]
enum CheckerError {
    Redis(redis::RedisError),
    Nats(async_nats::Error),
}

impl fmt::Display for CheckerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CheckerError::Redis(e) => write!(f, "Redis error: {}", e),
            CheckerError::Nats(e) => write!(f, "NATS error: {}", e),
        }
    }
}

impl Error for CheckerError {}

impl From<redis::RedisError> for CheckerError {
    fn from(err: redis::RedisError) -> CheckerError {
        CheckerError::Redis(err)
    }
}

impl From<async_nats::Error> for CheckerError {
    fn from(err: async_nats::Error) -> CheckerError {
        CheckerError::Nats(err)
    }
}

#[derive(Debug, Deserialize)]
struct CheckRequest {
    #[allow(dead_code)]
    host: String,
    #[allow(dead_code)]
    port: u16,
    #[allow(dead_code)]
    check_id: Option<String>,
    #[allow(dead_code)]
    user_submitted: Option<bool>,
}

#[allow(dead_code)]
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

#[derive(Clone)]
struct Worker {
    nats: async_nats::Client,
    redis: redis::Client,
}

impl Worker {
    async fn new() -> Result<Self, Box<dyn Error + Send + Sync>> {
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

        let nats = async_nats::connect(&nats_url).await?;
        let redis = redis::Client::open(redis_url.as_str())?;

        Ok(Worker { nats, redis })
    }

    async fn process_check(&self, msg: async_nats::Message) {
        let _check_request: CheckRequest = match serde_json::from_slice(&msg.payload) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Failed to parse check request: {e}");
                return;
            }
        };

        // Fetch data from all sources concurrently
        let (
            blockstream_result,
            zecrocks_result,
            blockchair_result,
            blockchaindotcom_result,
            zcashexplorer_result
        ): (
            Result<HashMap<String, BlockchainInfo>, Box<dyn Error + Send + Sync>>,
            Result<HashMap<String, BlockchainInfo>, Box<dyn Error + Send + Sync>>,
            Result<HashMap<String, BlockchainInfo>, Box<dyn Error + Send + Sync>>,
            Result<HashMap<String, BlockchainInfo>, Box<dyn Error + Send + Sync>>,
            Result<HashMap<String, BlockchainInfo>, Box<dyn Error + Send + Sync>>
        ) = tokio::join!(
            blockstream::get_blockchain_info(),
            zecrocks::get_blockchain_info(),
            blockchair::get_blockchain_info(),
            blockchaindotcom::get_blockchain_info(),
            zcashexplorer::get_blockchain_info()
        );

        let mut con = match self.redis.get_connection() {
            Ok(con) => con,
            Err(e) => {
                eprintln!("Failed to get Redis connection: {e}");
                return;
            }
        };

        // Process results and store in Redis
        let results = [
            ("blockstream", blockstream_result),
            ("zecrocks", zecrocks_result),
            ("blockchair", blockchair_result),
            ("blockchain", blockchaindotcom_result),
            ("zcashexplorer", zcashexplorer_result),
        ];

        for (source, result) in results {
            if let Ok(data) = result {
                for (_chain, info) in data {
                    if let Some(height) = info.height {
                        println!("{} height for {}: {} ({})", source, info.name, height, info.symbol);
                        let key = format!("http:{}.{}", source, info.symbol.to_lowercase());
                        let _: Result<(), _> = con.set(key, height);
                    }
                }
            }
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let nats_prefix = std::env::var("NATS_PREFIX").unwrap_or_else(|_| "hosh.".into());
        let mut sub = self.nats.subscribe(format!("{}check.http", nats_prefix)).await?;
        println!("Subscribed to {}check.http", nats_prefix);

        while let Some(msg) = sub.next().await {
            self.process_check(msg).await;
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let worker = Worker::new().await?;
    worker.run().await
}
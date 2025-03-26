use redis::Commands;
use std::error::Error;
use std::fmt;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::time::Instant;
use tracing::{debug, error, info, warn};

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

#[derive(Clone)]
struct Worker {
    nats: async_nats::Client,
    redis: redis::Client,
}

impl Worker {
    async fn new() -> Result<Self, Box<dyn Error>> {
        info!("==========================================");
        info!("HTTP checker starting up...");
        info!("==========================================");

        // Get NATS configuration
        let nats_url = format!(
            "nats://{}:{}",
            std::env::var("NATS_HOST").unwrap_or_else(|_| "nats".into()),
            std::env::var("NATS_PORT").unwrap_or_else(|_| "4222".into())
        );
        let nats_user = std::env::var("NATS_USERNAME").unwrap_or_default();
        let nats_password = std::env::var("NATS_PASSWORD").unwrap_or_default();
        let nats_prefix = std::env::var("NATS_PREFIX").unwrap_or_else(|_| "hosh.".into());

        let nats = if !nats_user.is_empty() && !nats_password.is_empty() {
            info!("Connecting to NATS with authentication...");
            let client = async_nats::ConnectOptions::new()
                .user_and_password(nats_user.clone(), nats_password)
                .connect(&nats_url)
                .await?;
            info!("✅ Successfully authenticated with NATS using username: {}", nats_user);
            client
        } else {
            info!("Connecting to NATS without authentication...");
            let client = async_nats::connect(&nats_url).await?;
            info!("✅ Successfully connected to NATS");
            client
        };

        // Verify connection by publishing and receiving a test message
        let test_subject = format!("{}test.{}", nats_prefix, uuid::Uuid::new_v4());
        info!("Testing connection with subject: {}", test_subject);
        let test_payload = "connection_test";
        
        info!("Creating test subscription...");
        let mut sub = match nats.subscribe(test_subject.clone()).await {
            Ok(sub) => {
                info!("Successfully created test subscription");
                sub
            },
            Err(e) => {
                error!("Failed to create test subscription: {}", e);
                return Err(Box::new(CheckerError::Other(e.to_string())));
            }
        };

        if let Err(e) = nats.publish(test_subject, test_payload.into()).await {
            error!("Failed to publish test message: {}", e);
            return Err(Box::new(CheckerError::Other(e.to_string())));
        }
        info!("Test message published");

        // Test the connection with timeout
        let timeout_duration = std::time::Duration::from_secs(5);
        match tokio::time::timeout(timeout_duration, sub.next()).await {
            Ok(Some(msg)) => {
                if msg.payload == test_payload.as_bytes() {
                    info!("✅ NATS connection verified with test message");
                } else {
                    warn!("⚠️ NATS test message received but payload mismatch");
                }
            },
            Ok(None) => warn!("⚠️ NATS subscription closed unexpectedly"),
            Err(_) => warn!("⚠️ NATS test message timeout - connection may be unstable"),
        }

        // Cleanup test subscription
        drop(sub);

        let redis_url = format!(
            "redis://{}:{}",
            std::env::var("REDIS_HOST").unwrap_or_else(|_| "redis".into()),
            std::env::var("REDIS_PORT").unwrap_or_else(|_| "6379".into())
        );

        let redis = redis::Client::open(redis_url.as_str())?;
        info!("Connected to Redis at {}", redis_url);

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
        let (blockstream_result, zecrocks_result, blockchair_result, blockchain_result, zcashexplorer_result) = tokio::join!(
            blockstream::get_blockchain_info(),
            zecrocks::get_blockchain_info(),
            blockchair::get_blockchain_info(),
            blockchain::get_blockchain_info(),
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
            ("blockchain", blockchain_result),
            ("zcashexplorer", zcashexplorer_result),
        ];

        for (source, result) in results {
            if let Ok(data) = result {
                for (chain, info) in data {
                    if let Some(height) = info.height {
                        println!("{} height: {} ({})", info.name, height, source);
                        let _: Result<(), _> = con.set(
                            format!("http:{}.{}", source, chain),
                            height
                        );
                    }
                }
            }
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn Error>> {
        let nats_prefix = std::env::var("NATS_PREFIX").unwrap_or_else(|_| "hosh.".into());
        let subject = format!("{}check.http", nats_prefix);
        
        info!("🎯 Attempting to subscribe to NATS subject: {}", subject);
        let mut sub = self.nats.subscribe(subject.clone()).await?;
        info!("✅ Successfully subscribed to {}", subject);
        info!("👂 Listening for HTTP check requests...");

        while let Some(msg) = sub.next().await {
            info!("📥 Received message on subject: {}", subject);
            self.process_check(msg).await;
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    let worker = Worker::new().await?;
    worker.run().await
}
use redis::Commands;
use std::env;
use std::error::Error;
use std::fmt;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::{self, Uuid};
use reqwest;
use tracing::{error, info};
use std::process::Command;
use tracing_subscriber;

mod blockchair;
mod blockchaindotcom;
mod blockstream;
mod mempool;
mod zecrocks;
mod zcashexplorer;
mod types;

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
    #[serde(default)]
    url: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default)]
    check_id: Option<String>,
    #[serde(default)]
    user_submitted: Option<bool>,
    #[serde(default)]
    dry_run: bool,
}

fn default_port() -> u16 { 80 }

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
struct ClickhouseConfig {
    url: String,
    user: String,
    password: String,
    database: String,
}

impl ClickhouseConfig {
    fn from_env() -> Self {
        Self {
            url: format!("http://{}:{}", 
                env::var("CLICKHOUSE_HOST").unwrap_or_else(|_| "chronicler".into()),
                env::var("CLICKHOUSE_PORT").unwrap_or_else(|_| "8123".into())
            ),
            user: env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "hosh".into()),
            password: env::var("CLICKHOUSE_PASSWORD").unwrap_or_else(|_| "chron".into()),
            database: env::var("CLICKHOUSE_DB").unwrap_or_else(|_| "hosh".into()),
        }
    }
}

#[derive(Clone)]
struct Worker {
    nats: async_nats::Client,
    redis: redis::Client,
    clickhouse: ClickhouseConfig,
    http_client: reqwest::Client,
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

        println!("Redis key format: http:{{source}}.{{chain}}");
        println!("Example: http:blockchair.bitcoin, http:blockchain.bitcoin");

        let nats = async_nats::connect(&nats_url).await?;
        let redis = redis::Client::open(redis_url.as_str())?;

        let http_client = reqwest::Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(300))
            .pool_max_idle_per_host(32)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Worker { nats, redis, clickhouse: ClickhouseConfig::from_env(), http_client })
    }

    async fn process_check(&self, msg: async_nats::Message) {
        let check_request: CheckRequest = match serde_json::from_slice(&msg.payload) {
            Ok(req) => req,
            Err(e) => {
                error!("Failed to parse check request: {e}");
                return;
            }
        };

        // Check if dry run mode is enabled
        if check_request.dry_run {
            info!(
                "DRY RUN: Received check request - url={} port={} check_id={} user_submitted={}",
                check_request.url,
                check_request.port,
                check_request.check_id.as_deref().unwrap_or("none"),
                check_request.user_submitted.unwrap_or(false)
            );
            return;
        }

        let mut con = match self.redis.get_connection() {
            Ok(con) => con,
            Err(e) => {
                error!("Failed to get Redis connection: {e}");
                return;
            }
        };

        info!("Starting concurrent blockchain info fetching...");
        
        // Group explorers by domain
        let explorers = vec![
            // Blockchair (clearnet and onion)
            ("blockchair", blockchair::get_blockchain_info().await),
            ("blockchair-onion", blockchair::get_onion_blockchain_info().await),
            // Other explorers (clearnet only)
            ("blockstream", blockstream::get_blockchain_info().await),
            ("zecrocks", zecrocks::get_blockchain_info().await),
            ("blockchain", blockchaindotcom::get_blockchain_info().await),
            ("zcashexplorer", zcashexplorer::get_blockchain_info().await),
        ];

        let mut total_insertions = 0;
        let mut total_errors = 0;

        // Process all results
        for (source, result) in explorers {
            match result {
                Ok(data) => {
                    for (chain_id, info) in data {
                        if let Some(height) = info.height {
                            let redis_key = format!("http:{}.{}", source, chain_id);
                            match con.set::<_, _, ()>(&redis_key, height) {
                                Ok(_) => {
                                    let result = CheckResult {
                                        host: format!("{}.{}", source, chain_id),
                                        port: check_request.port,
                                        height,
                                        status: "online".to_string(),
                                        error: None,
                                        last_updated: Utc::now(),
                                        ping: 0.0,
                                        check_id: check_request.check_id.clone(),
                                        user_submitted: check_request.user_submitted,
                                    };
                                    
                                    if let Err(e) = self.publish_to_clickhouse(source, &chain_id, &result).await {
                                        error!("Failed to publish to ClickHouse for {}.{}: {}", source, chain_id, e);
                                        total_errors += 1;
                                    } else {
                                        total_insertions += 1;
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to set Redis key {}: {}", redis_key, e);
                                    total_errors += 1;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to fetch from {}: {}", source, e);
                    total_errors += 1;
                }
            }
        }

        info!(
            total_insertions = total_insertions,
            total_errors = total_errors,
            "Completed HTTP check cycle"
        );
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

    async fn publish_to_clickhouse(&self, source: &str, chain_id: &str, result: &CheckResult) -> Result<(), Box<dyn Error>> {
        info!("📊 Publishing to ClickHouse for {}.{}", source, chain_id);
        
        let target_id = Uuid::new_v5(
            &Uuid::NAMESPACE_DNS,
            format!("http:{}.{}", source, chain_id).as_bytes()
        ).to_string();

        let escaped_host = format!("{}.{}", source, chain_id).replace("'", "\\'");
        
        // Update existing target
        info!("Updating target in ClickHouse: {}", escaped_host);
        let update_query = format!(
            "ALTER TABLE {db}.targets 
             UPDATE last_queued_at = now(),
                    last_checked_at = now(),
                    target_id = '{target_id}'
             WHERE module = 'http' AND hostname = '{host}'
             SETTINGS mutations_sync = 1",
            db = self.clickhouse.database,
            target_id = target_id,
            host = escaped_host,
        );

        let response = self.http_client.post(&self.clickhouse.url)
            .basic_auth(&self.clickhouse.user, Some(&self.clickhouse.password))
            .header("Content-Type", "text/plain")
            .body(update_query.clone())
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("❌ ClickHouse update failed: {}", error_text);
            return Err(format!("ClickHouse update error: {}", error_text).into());
        }
        info!("✅ Target update successful");

        // Insert new target if it doesn't exist
        info!("Inserting new target if not exists: {}", escaped_host);
        let insert_query = format!(
            "INSERT INTO {db}.targets (target_id, module, hostname, last_queued_at, last_checked_at, user_submitted)
             SELECT '{target_id}', 'http', '{host}', now(), now(), {user_submitted}
             WHERE NOT EXISTS (
                 SELECT 1 FROM {db}.targets 
                 WHERE module = 'http' AND hostname = '{host}'
             )",
            db = self.clickhouse.database,
            target_id = target_id,
            host = escaped_host,
            user_submitted = result.user_submitted.unwrap_or(false)
        );

        let response = self.http_client.post(&self.clickhouse.url)
            .basic_auth(&self.clickhouse.user, Some(&self.clickhouse.password))
            .header("Content-Type", "text/plain")
            .body(insert_query.clone())
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("❌ ClickHouse insert failed: {}", error_text);
            return Err(format!("ClickHouse insert error: {}", error_text).into());
        }
        info!("✅ Target insert successful");

        // Insert the check result
        info!("Inserting check result for {}.{} (height: {})", source, chain_id, result.height);
        let result_query = format!(
            "INSERT INTO {}.results 
             (target_id, checked_at, hostname, resolved_ip, ip_version, 
              checker_module, status, ping_ms, checker_location, checker_id, response_data, user_submitted) 
             VALUES 
             ('{}', now(), '{}', '', 4, 'http', '{}', {}, 'default', '{}', '{}', {})",
            self.clickhouse.database,
            target_id,
            escaped_host,
            if result.error.is_some() { "offline" } else { "online" },
            result.ping,
            Uuid::new_v4(),
            serde_json::to_string(&result)?.replace("'", "\\'"),
            result.user_submitted.unwrap_or(false)
        );

        let response = self.http_client.post(&self.clickhouse.url)
            .basic_auth(&self.clickhouse.user, Some(&self.clickhouse.password))
            .header("Content-Type", "text/plain")
            .body(result_query.clone())
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("❌ ClickHouse result insert failed: {}", error_text);
            return Err(format!("ClickHouse error: {}", error_text).into());
        }

        info!(
            "✅ Successfully published to ClickHouse: url={} chain={} height={} check_id={}",
            format!("http:{}.{}", source, chain_id),
            chain_id,
            result.height,
            result.check_id.as_deref().unwrap_or("none")
        );

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false)
        .with_ansi(true)
        .init();

    // Clear screen based on platform
    if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "cls"])
            .status()
            .expect("Failed to clear screen");
    } else {
        Command::new("clear")
            .status()
            .expect("Failed to clear screen");
    }

    info!("🔍 HTTP Block Explorer Checker Starting...");
    info!("==========================================\n");

    info!("Testing Tor connection...");
    if let Ok(_client) = blockchair::blockchairdotonion::create_client() {
        info!("✅ Successfully created Tor client");
    } else {
        error!("❌ Failed to create Tor client");
    }

    // Add ClickHouse connection test
    let worker = Worker::new().await?;
    
    // Test ClickHouse connection
    info!("Testing ClickHouse connection...");
    let test_query = "SELECT 1";
    let response = worker.http_client.post(&worker.clickhouse.url)
        .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
        .header("Content-Type", "text/plain")
        .body(test_query)
        .send()
        .await?;

    if response.status().is_success() {
        info!("✅ Successfully connected to ClickHouse at {}", worker.clickhouse.url);
    } else {
        error!("❌ Failed to connect to ClickHouse: {}", response.status());
        error!("Response: {}", response.text().await?);
    }

    // Log ClickHouse configuration
    info!("ClickHouse configuration:");
    info!("  URL: {}", worker.clickhouse.url);
    info!("  Database: {}", worker.clickhouse.database);
    info!("  User: {}", worker.clickhouse.user);

    worker.run().await
}
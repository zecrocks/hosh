use redis::Commands;
use std::error::Error;
use std::fmt;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use futures::try_join;
use std::env;
use uuid;
use reqwest;
use tracing::{error, info};
use uuid::Uuid;

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
    #[allow(dead_code)]
    url: String,
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
            .build()?;

        Ok(Worker { nats, redis, clickhouse: ClickhouseConfig::from_env(), http_client })
    }

    async fn process_check(&self, msg: async_nats::Message) {
        let _check_request: CheckRequest = match serde_json::from_slice(&msg.payload) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Failed to parse check request: {e}");
                return;
            }
        };

        let mut con = match self.redis.get_connection() {
            Ok(con) => con,
            Err(e) => {
                eprintln!("Failed to get Redis connection: {e}");
                return;
            }
        };

        let results = match try_join!(
            blockstream::get_blockchain_info(),
            zecrocks::get_blockchain_info(),
            blockchair::get_blockchain_info(),
            blockchair::get_onion_blockchain_info(),
            blockchaindotcom::get_blockchain_info(),
            zcashexplorer::get_blockchain_info()
        ) {
            Ok((blockstream, zecrocks, blockchair, blockchair_onion, blockchain, zcashexplorer)) => {
                vec![
                    ("blockstream", blockstream),
                    ("zecrocks", zecrocks),
                    ("blockchair", blockchair),
                    ("blockchair-onion", blockchair_onion),
                    ("blockchain", blockchain),
                    ("zcashexplorer", zcashexplorer),
                ]
            }
            Err(e) => {
                eprintln!("Error during concurrent fetching: {}", e);
                vec![]
            }
        };

        // Process results immediately
        for (source, data) in results {
            for (chain_id, info) in data {
                if let Some(height) = info.height {
                    let redis_key = format!("http:{}.{}", source, chain_id);
                    match con.set::<_, _, ()>(&redis_key, height) {
                        Ok(_) => println!("📝 {} height: {} ({})", info.name, height, source),
                        Err(e) => eprintln!("Failed to set Redis key {}: {}", redis_key, e),
                    }
                }
            }
        }

        let result = CheckResult {
            host: _check_request.url.clone(),
            port: _check_request.port,
            height: 0,
            status: "online".to_string(),
            error: None,
            last_updated: Utc::now(),
            ping: 0.0,
            check_id: _check_request.check_id.clone(),
            user_submitted: _check_request.user_submitted,
        };

        if let Err(e) = self.publish_to_clickhouse(&_check_request, &result).await {
            error!("Failed to publish to ClickHouse: {}", e);
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

    async fn publish_to_clickhouse(&self, check_request: &CheckRequest, result: &CheckResult) -> Result<(), Box<dyn Error>> {
        let target_id = uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_DNS,
            format!("http:{}", check_request.url).as_bytes()
        ).to_string();

        let escaped_url = check_request.url.replace("'", "\\'");
        
        // Update existing target
        let update_query = format!(
            "ALTER TABLE {db}.targets 
             UPDATE last_queued_at = now(),
                    last_checked_at = now(),
                    target_id = '{target_id}'
             WHERE module = 'http' AND hostname = '{url}'
             SETTINGS mutations_sync = 1",
            db = self.clickhouse.database,
            target_id = target_id,
            url = escaped_url,
        );

        let response = self.http_client.post(&self.clickhouse.url)
            .basic_auth(&self.clickhouse.user, Some(&self.clickhouse.password))
            .header("Content-Type", "text/plain")
            .body(update_query)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("ClickHouse update error: {}", response.text().await?).into());
        }

        // Insert new target if it doesn't exist
        let insert_query = format!(
            "INSERT INTO {db}.targets (target_id, module, hostname, last_queued_at, last_checked_at, user_submitted)
             SELECT '{target_id}', 'http', '{url}', now(), now(), {user_submitted}
             WHERE NOT EXISTS (
                 SELECT 1 FROM {db}.targets 
                 WHERE module = 'http' AND hostname = '{url}'
             )",
            db = self.clickhouse.database,
            target_id = target_id,
            url = escaped_url,
            user_submitted = check_request.user_submitted.unwrap_or(false)
        );

        let response = self.http_client.post(&self.clickhouse.url)
            .basic_auth(&self.clickhouse.user, Some(&self.clickhouse.password))
            .header("Content-Type", "text/plain")
            .body(insert_query)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("ClickHouse insert error: {}", response.text().await?).into());
        }

        // Insert the check result
        let result_query = format!(
            "INSERT INTO {}.results 
             (target_id, checked_at, hostname, resolved_ip, ip_version, 
              checker_module, status, ping_ms, checker_location, checker_id, response_data, user_submitted) 
             VALUES 
             ('{}', now(), '{}', '', 4, 'http', '{}', {}, 'default', '{}', '{}', {})",
            self.clickhouse.database,
            target_id,
            escaped_url,
            if result.error.is_some() { "offline" } else { "online" },
            result.ping,
            uuid::Uuid::new_v4(),
            serde_json::to_string(&result)?.replace("'", "\\'"),
            check_request.user_submitted.unwrap_or(false)
        );

        let response = self.http_client.post(&self.clickhouse.url)
            .basic_auth(&self.clickhouse.user, Some(&self.clickhouse.password))
            .header("Content-Type", "text/plain")
            .body(result_query)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("ClickHouse error: {}", response.text().await?).into());
        }

        info!(
            url = %check_request.url,
            check_id = %check_request.check_id.as_deref().unwrap_or("none"),
            "Successfully saved check data to ClickHouse"
        );

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("Testing Tor connection...");
    if let Ok(_client) = blockchair::blockchairdotonion::create_client() {
        println!("Successfully created Tor client");
    } else {
        println!("Failed to create Tor client");
    }

    let worker = Worker::new().await?;
    worker.run().await
}
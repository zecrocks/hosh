use std::{env, error::Error, time::Duration};
use redis::Commands;
use serde::{Deserialize, Serialize};
use tokio::time;
use chrono::{DateTime, Utc};
use tracing::{info, error, debug};
use uuid::Uuid;
use reqwest::Client;
use env_logger;
use log;
use std::io::Write;

// Environment variable constants
const DEFAULT_DISCOVERY_INTERVAL: u64 = 3600; // 1 hour default
const FORCE_REFRESH: bool = true; // Set to true to force refresh all servers

// ClickHouse configuration
struct ClickHouseConfig {
    url: String,
    user: String,
    password: String,
    database: String,
}

impl ClickHouseConfig {
    fn from_env() -> Self {
        Self {
            url: format!("http://{}:{}", 
                env::var("CLICKHOUSE_HOST").unwrap_or_else(|_| "chronicler".into()),
                env::var("CLICKHOUSE_PORT").unwrap_or_else(|_| "8123".into())
            ),
            user: env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "hosh".into()),
            password: env::var("CLICKHOUSE_PASSWORD").expect("CLICKHOUSE_PASSWORD environment variable must be set"),
            database: env::var("CLICKHOUSE_DB").unwrap_or_else(|_| "hosh".into()),
        }
    }
}

// Static ZEC server configuration
const ZEC_SERVERS: &[(&str, u16)] = &[
    ("zec.rocks", 443),
    ("na.zec.rocks", 443),
    ("sa.zec.rocks", 443),
    ("eu.zec.rocks", 443),
    ("ap.zec.rocks", 443),
    ("me.zec.rocks", 443),
    ("testnet.zec.rocks", 443),
    ("zcashd.zec.rocks", 443),
    ("zaino.unsafe.zec.rocks", 443),
    ("zaino.testnet.unsafe.zec.rocks", 443),
    ("lwd1.zcash-infra.com", 9067),
    ("lwd2.zcash-infra.com", 9067),
    ("lwd3.zcash-infra.com", 9067),
    ("lwd4.zcash-infra.com", 9067),
    ("lwd5.zcash-infra.com", 9067),
    ("lwd6.zcash-infra.com", 9067),
    ("lwd7.zcash-infra.com", 9067),
    ("lwd8.zcash-infra.com", 9067),
];

#[derive(Debug, Deserialize)]
struct BtcServerDetails {
    #[serde(default)]
    s: Option<String>,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ServerData {
    host: String,
    port: u16,
    #[serde(default)]
    height: u64,
    #[serde(default)]
    status: String,
    error: Option<String>,
    last_updated: DateTime<Utc>,
    #[serde(default)]
    ping: f64,
    #[serde(default)]
    version: Option<String>,
}

async fn fetch_btc_servers() -> Result<std::collections::HashMap<String, BtcServerDetails>, Box<dyn Error>> {
    info!("Fetching BTC servers from Electrum repository...");
    let client = reqwest::Client::new();
    let response = client
        .get("https://raw.githubusercontent.com/spesmilo/electrum/refs/heads/master/electrum/chains/servers.json")
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    
    let servers: std::collections::HashMap<String, BtcServerDetails> = response.json().await?;
    info!("Found {} BTC servers", servers.len());
    Ok(servers)
}

async fn update_servers(redis_client: redis::Client, clickhouse: ClickHouseConfig, http_client: Client) -> Result<(), Box<dyn Error>> {
    let mut conn = redis_client.get_connection()?;
    let mut btc_count = 0;
    let mut zec_count = 0;
    
    loop {        
        info!("Starting discovery cycle...");
        
        match fetch_btc_servers().await {
            Ok(btc_servers) => {
                for (host, details) in btc_servers {
                    let redis_key = format!("btc:{}", host);
                    let exists = conn.exists::<_, bool>(&redis_key)?;
                    
                    if !exists || FORCE_REFRESH {
                        let port = details.s
                            .and_then(|s| s.parse::<u16>().ok())
                            .unwrap_or(50002);

                        let server_data = ServerData {
                            host: host.clone(),
                            port,
                            height: 0,
                            status: "new".to_string(),
                            error: None,
                            last_updated: Utc::now(),
                            ping: 0.0,
                            version: details.version,
                        };
                        
                        let json = serde_json::to_string(&server_data)?;
                        conn.set::<_, _, ()>(&redis_key, json)?;
                        debug!("Added/Updated BTC server: {}:{}", host, port);
                        btc_count += 1;

                        // Publish to ClickHouse
                        let target_id = Uuid::new_v5(
                            &Uuid::NAMESPACE_DNS,
                            format!("btc:{}", host).as_bytes()
                        ).to_string();

                        let escaped_host = host.replace("'", "\\'");
                        
                        // Update existing target or create new one
                        let upsert_query = format!(
                            "INSERT INTO {db}.targets (target_id, module, hostname, last_queued_at, last_checked_at, user_submitted)
                             SELECT '{target_id}', 'btc', '{host}', now(), now(), false
                             WHERE NOT EXISTS (
                                 SELECT 1 FROM {db}.targets 
                                 WHERE module = 'btc' AND hostname = '{host}'
                             )",
                            db = clickhouse.database,
                            target_id = target_id,
                            host = escaped_host,
                        );

                        let response = http_client.post(&clickhouse.url)
                            .basic_auth(&clickhouse.user, Some(&clickhouse.password))
                            .header("Content-Type", "text/plain")
                            .body(upsert_query)
                            .send()
                            .await?;

                        if !response.status().is_success() {
                            error!("Failed to insert BTC target into ClickHouse: {}", response.text().await?);
                        }
                    } else {
                        debug!("BTC server already exists: {}:{}", host, details.s.as_deref().unwrap_or("50002"));
                    }
                }
            }
            Err(e) => error!("Error fetching BTC servers: {}", e),
        }

        info!("Processing {} ZEC servers...", ZEC_SERVERS.len());
        for (host, port) in ZEC_SERVERS {
            let redis_key = format!("zec:{}", host);
            let exists = conn.exists::<_, bool>(&redis_key)?;

            if !exists || FORCE_REFRESH {
                let server_data = ServerData {
                    host: host.to_string(),
                    port: *port,
                    height: 0,
                    status: "new".to_string(),
                    error: None,
                    last_updated: Utc::now(),
                    ping: 0.0,
                    version: None,
                };
                
                let json = serde_json::to_string(&server_data)?;
                conn.set::<_, _, ()>(&redis_key, json)?;
                debug!("Added/Updated ZEC server: {}:{}", host, port);
                zec_count += 1;

                // Publish to ClickHouse
                let target_id = Uuid::new_v5(
                    &Uuid::NAMESPACE_DNS,
                    format!("zec:{}", host).as_bytes()
                ).to_string();

                let escaped_host = host.replace("'", "\\'");
                
                // Update existing target or create new one
                let upsert_query = format!(
                    "INSERT INTO {db}.targets (target_id, module, hostname, last_queued_at, last_checked_at, user_submitted)
                     SELECT '{target_id}', 'zec', '{host}', now(), now(), false
                     WHERE NOT EXISTS (
                         SELECT 1 FROM {db}.targets 
                         WHERE module = 'zec' AND hostname = '{host}'
                     )",
                    db = clickhouse.database,
                    target_id = target_id,
                    host = escaped_host,
                );

                let response = http_client.post(&clickhouse.url)
                    .basic_auth(&clickhouse.user, Some(&clickhouse.password))
                    .header("Content-Type", "text/plain")
                    .body(upsert_query)
                    .send()
                    .await?;

                if !response.status().is_success() {
                    error!("Failed to insert ZEC target into ClickHouse: {}", response.text().await?);
                }
            } else {
                debug!("ZEC server already exists: {}:{}", host, port);
            }
        }

        // Print current server counts from Redis
        let btc_keys: Vec<String> = redis::cmd("KEYS")
            .arg("btc:*")
            .query(&mut conn)?;
        let zec_keys: Vec<String> = redis::cmd("KEYS")
            .arg("zec:*")
            .query(&mut conn)?;
        
        info!(
            btc_servers = btc_count,
            zec_servers = zec_count,
            total_btc_servers = btc_keys.len(),
            total_zec_servers = zec_keys.len(),
            "Discovery cycle complete. Added/Updated {} BTC and {} ZEC servers. Total servers in Redis: {} BTC, {} ZEC. Sleeping for {} seconds", 
            btc_count,
            zec_count,
            btc_keys.len(),
            zec_keys.len(),
            DEFAULT_DISCOVERY_INTERVAL
        );
        
        time::sleep(Duration::from_secs(DEFAULT_DISCOVERY_INTERVAL)).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging with more detailed format
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Debug)
        .filter_module("hyper", log::LevelFilter::Warn)  // Reduce hyper verbosity
        .filter_module("reqwest", log::LevelFilter::Warn)  // Reduce reqwest verbosity
        .format(|buf, record| {
            writeln!(
                buf,
                "{} {} {}:{}: {}",
                chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S.%fZ"),
                record.level(),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .init();

    info!("Starting discovery service...");

    let redis_host = env::var("REDIS_HOST").unwrap_or_else(|_| "redis".to_string());
    let redis_port = env::var("REDIS_PORT").unwrap_or_else(|_| "6379".to_string());
    
    let redis_url = format!("redis://{}:{}", redis_host, redis_port);
    info!("Connecting to Redis at {}:{}", redis_host, redis_port);
    
    let redis_client = redis::Client::open(redis_url.as_str())?;
    
    // Test Redis connection
    let mut conn = redis_client.get_connection()?;
    redis::cmd("PING").query::<String>(&mut conn)?;
    info!("Successfully connected to Redis");

    // Initialize ClickHouse client
    let clickhouse = ClickHouseConfig::from_env();
    let http_client = Client::new();
    info!("Initialized ClickHouse client");

    update_servers(redis_client, clickhouse, http_client).await?;

    Ok(())
} 
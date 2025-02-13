use std::{env, error::Error, time::Duration};
use redis::Commands;
use serde::{Deserialize, Serialize};
use tokio::time;
use chrono::{DateTime, Utc};
use tracing::{info, error};
use redis::RedisResult;

// Environment variable constants
const DEFAULT_DISCOVERY_INTERVAL: u64 = 3600; // 1 hour default

// Static ZEC server configuration
const ZEC_SERVERS: &[(&str, u16)] = &[
    ("zec.rocks", 443),
    ("na.zec.rocks", 443),
    ("sa.zec.rocks", 443),
    ("eu.zec.rocks", 443),
    ("ap.zec.rocks", 443),
    ("me.zec.rocks", 443),
    ("zcashd.zec.rocks", 443),
    ("lwd1.zcash-infra.com", 9067),
    ("lwd2.zcash-infra.com", 9067),
    ("lwd3.zcash-infra.com", 9067),
    ("lwd4.zcash-infra.com", 9067),
    ("lwd5.zcash-infra.com", 9067),
    ("lwd6.zcash-infra.com", 9067),
    ("lwd7.zcash-infra.com", 9067),
    ("lwd8.zcash-infra.com", 9067),
];

#[derive(Debug, Serialize, Deserialize)]
struct ServerData {
    host: String,
    port: u16,
    #[serde(default)]
    height: u64,
    #[serde(default)]
    status: String,
    error: Option<String>,
    #[serde(rename = "LastUpdated")]
    last_updated: DateTime<Utc>,
    #[serde(default)]
    ping: f64,
}

#[derive(Debug, Deserialize)]
struct BtcServerDetails {
    s: Option<u16>,
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BtcServersResponse {
    servers: std::collections::HashMap<String, BtcServerDetails>,
}

async fn fetch_btc_servers() -> Result<std::collections::HashMap<String, BtcServerDetails>, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://raw.githubusercontent.com/spesmilo/electrum/refs/heads/master/electrum/chains/servers.json")
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    
    let servers: BtcServersResponse = response.json().await?;
    Ok(servers.servers)
}

async fn update_servers(redis_client: redis::Client) -> Result<(), Box<dyn Error>> {
    let mut conn = redis_client.get_connection()?;
    
    loop {        
        match fetch_btc_servers().await {
            Ok(btc_servers) => {
                for (host, details) in btc_servers {
                    let redis_key = format!("btc:{}", host);
                    if !conn.exists::<_, bool>(&redis_key)? {
                        let server_data = ServerData {
                            host: host.clone(),
                            port: details.s.unwrap_or(50002),
                            height: 0,
                            status: "new".to_string(),
                            error: None,
                            last_updated: Utc::now(),
                            ping: 0.0,
                        };
                        
                        let json = serde_json::to_string(&server_data)?;
                        conn.set::<_, _, ()>(&redis_key, json)?;
                    }
                }
            }
            Err(e) => error!("Error fetching BTC servers: {}", e),
        }

        for (host, port) in ZEC_SERVERS {
            let redis_key = format!("zec:{}", host);
            let exists = conn.exists::<_, bool>(&redis_key)?;

            if !exists {
                let server_data = ServerData {
                    host: host.to_string(),
                    port: *port,
                    height: 0,
                    status: "new".to_string(),
                    error: None,
                    last_updated: Utc::now(),
                    ping: 0.0,
                };
                
                let json = serde_json::to_string(&server_data)?;
                conn.set::<_, _, ()>(&redis_key, json)?;
            }
        }

        let interval = env::var("DISCOVERY_INTERVAL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_DISCOVERY_INTERVAL);
        
        info!("Discovery cycle complete. Sleeping for {} seconds", interval);
        time::sleep(Duration::from_secs(interval)).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let redis_host = env::var("REDIS_HOST").unwrap_or_else(|_| "redis".to_string());
    let redis_port = env::var("REDIS_PORT").unwrap_or_else(|_| "6379".to_string());
    
    let redis_url = format!("redis://{}:{}", redis_host, redis_port);
    let redis_client = redis::Client::open(redis_url.as_str())?;
    
    // Test Redis connection
    let mut conn = redis_client.get_connection()?;
    redis::cmd("PING").query::<String>(&mut conn)?;

    update_servers(redis_client).await?;

    Ok(())
} 
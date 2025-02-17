use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::{Deserialize, de::Error as SerdeError, Deserializer, Serialize};
use serde_json::Value;
use std::env;
use std::time::Duration;
use tokio::time;

// Custom deserializer to allow numbers or strings (or null) to become Option<String>
fn int_or_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let val = Value::deserialize(deserializer)?;
    Ok(match val {
        Value::String(s) => Some(s),
        Value::Number(n) => Some(n.to_string()),
        Value::Null => None,
        _ => return Err(SerdeError::custom("expected string or number")),
    })
}

// Custom deserializer for port, allowing a numeric or string JSON value to become u16
fn port_or_string<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: Deserializer<'de>,
{
    let val = Value::deserialize(deserializer)?;
    match val {
        Value::Null => Ok(None),
        Value::Number(n) => {
            let num = n.as_u64().ok_or_else(|| SerdeError::custom("port must be a positive integer"))?;
            if num <= u16::MAX as u64 {
                Ok(Some(num as u16))
            } else {
                Err(SerdeError::custom("port out of range for u16"))
            }
        },
        Value::String(s) => {
            let parsed = s.parse::<u16>()
                .map_err(|_| SerdeError::custom("invalid string for port"))?;
            Ok(Some(parsed))
        },
        _ => Err(SerdeError::custom("expected a number or string for port")),
    }
}

// Add this new function near your other deserializers
fn deserialize_datetime<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    
    // Handle the "zero" datetime case
    if s == "0001-01-01T00:00:00" {
        return Ok(None);
    }
    
    // Try to parse the datetime
    match DateTime::parse_from_rfc3339(&s) {
        Ok(dt) => Ok(Some(dt.with_timezone(&Utc))),
        Err(_) => Ok(None),  // Return None for any parsing errors
    }
}

const DEFAULT_REFRESH_INTERVAL: u64 = 300;
const DEFAULT_NATS_PREFIX: &str = "hosh.";
const DEFAULT_REDIS_PORT: u16 = 6379;
const PREFIXES: &[&str] = &["btc:", "zec:"];

#[derive(Debug, Deserialize, Serialize)]
struct ServerData {
    #[serde(default, deserialize_with = "port_or_string")]
    port: Option<u16>,

    // Handle both strings and integers by converting to a string
    #[serde(default, deserialize_with = "int_or_string")]
    version: Option<String>,

    #[serde(rename = "electrum_version", default, deserialize_with = "int_or_string")]
    electrum_version: Option<String>,

    // Make LastUpdated optional and use a custom deserializer
    #[serde(rename = "LastUpdated", default, deserialize_with = "deserialize_datetime")]
    last_updated: Option<DateTime<Utc>>,

    #[serde(default)]
    user_submitted: bool,

    #[serde(default)]  // Make check_id optional with a default
    check_id: Option<String>,
}

#[derive(Debug)]
struct Config {
    refresh_interval: Duration,
    nats_url: String,
    nats_prefix: String,
    redis_host: String,
    redis_port: u16,
}

impl Config {
    fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let refresh_interval = Duration::from_secs(
            env::var("CHECK_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_REFRESH_INTERVAL),
        );

        let nats_url = env::var("NATS_URL")
            .unwrap_or_else(|_| "nats://nats:4222".into());

        let nats_prefix = env::var("NATS_PREFIX")
            .unwrap_or_else(|_| DEFAULT_NATS_PREFIX.into());

        let redis_host = env::var("REDIS_HOST")
            .unwrap_or_else(|_| "redis".into());

        let redis_port = env::var("REDIS_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_REDIS_PORT);

        Ok(Self {
            refresh_interval,
            nats_url,
            nats_prefix,
            redis_host,
            redis_port,
        })
    }
}

fn is_stale(data: &ServerData, stale_duration: Duration) -> bool {
    let last_updated = match data.last_updated {
        Some(updated) => updated,
        None => return true,
    };

    let now = Utc::now();

    // Special handling for user submitted entries
    if data.user_submitted {
        let age = now.signed_duration_since(last_updated);
        if age.num_seconds() < 60 {  // Skip if checked within last minute
            tracing::debug!("Skipping recently user-submitted check ({}s ago)", age.num_seconds());
            return false;
        }
    }

    // Normal staleness check
    let stale_time = last_updated + chrono::Duration::from_std(stale_duration).unwrap();
    now > stale_time
}

fn network_from_key(key: &str) -> &str {
    if key.starts_with("btc:") {
        "btc"
    } else if key.starts_with("zec:") {
        "zec"
    } else {
        unreachable!("Unhandled prefix in key: {key}")
    }
}

fn default_port_for_network(network: &str) -> u16 {
    match network {
        "btc" => 50002,
        "zec" => 9067,
        _ => unreachable!("Unknown network: {network}"),
    }
}

async fn publish_checks(
    nats: async_nats::Client,
    mut redis: redis::aio::MultiplexedConnection,
    config: &Config,
) -> Result<()> {
    let mut interval = time::interval(config.refresh_interval);
    
    loop {
        interval.tick().await;
        
        // Publish single HTTP check request
        let subject = format!("{}check.http", config.nats_prefix);
        let message = serde_json::json!({
            "type": "http",
            "host": "trigger",  // Dummy value since the checker knows what to do
            "port": 0,
            "user_submitted": false,
            "check_id": None::<String>
        });

        if let Err(e) = nats.publish(subject.clone(), message.to_string().into()).await {
            tracing::error!("Failed to publish HTTP check trigger: {}", e);
        } else {
            tracing::info!("Published HTTP check trigger");
        }

        // Then handle the regular BTC/ZEC checks from Redis
        for prefix in PREFIXES {
            let keys: Vec<String> = match redis.keys(format!("{prefix}*")).await {
                Ok(keys) => keys,
                Err(e) => {
                    tracing::error!("Failed to get Redis keys for prefix {}: {}", prefix, e);
                    continue;
                }
            };

            if keys.is_empty() {
                tracing::info!("No keys found for prefix {prefix}");
                continue;
            }

            for key in keys {
                let raw_data: String = match redis.get(&key).await {
                    Ok(data) => data,
                    Err(e) => {
                        tracing::error!("Failed to get Redis data for key {}: {}", key, e);
                        continue;
                    }
                };

                // Log the raw data when it's corrupted
                if let Err(e) = serde_json::from_str::<ServerData>(&raw_data) {
                    tracing::error!(
                        "Invalid JSON for key {}: {}\nRaw data: {:?}",
                        key, 
                        e,
                        raw_data
                    );
                    continue;
                }

                let data: ServerData = serde_json::from_str(&raw_data).unwrap(); // Safe because we checked above

                if !is_stale(&data, config.refresh_interval) {
                    tracing::debug!(%key, "Skipping recently checked server");
                    continue;
                }

                let network = network_from_key(&key);
                let host = key.split_once(':')
                    .map(|(_, host)| host)
                    .unwrap_or(&key);

                let port = data.port.unwrap_or_else(|| default_port_for_network(network));

                let subject = format!("{}check.{}", config.nats_prefix, network);
                let message = match network {
                    "btc" => serde_json::json!({
                        "type": network,
                        "host": host,
                        "port": port,
                        "version": data.version.as_deref().or(data.electrum_version.as_deref()).unwrap_or("unknown"),
                        "user_submitted": data.user_submitted,
                        "check_id": data.check_id
                    }),
                    "zec" => serde_json::json!({
                        "type": network,
                        "host": host,
                        "port": port,
                        "user_submitted": data.user_submitted,
                        "check_id": data.check_id
                    }),
                    _ => continue,
                };

                if let Err(e) = nats.publish(subject.clone(), message.to_string().into()).await {
                    tracing::error!("Failed to publish message for {}: {}", key, e);
                    continue;
                }

                tracing::info!(%key, %subject, "Published check request");
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_env()?;

    let redis_client = redis::Client::open(
        format!("redis://{}:{}", config.redis_host, config.redis_port)
    )?;
    
    let redis_conn = redis_client.get_multiplexed_async_connection()
        .await
        .context("Failed to connect to Redis")?;

    let nats = async_nats::connect(&config.nats_url)
        .await
        .context("Failed to connect to NATS")?;

    tracing::info!("Connected to Redis and NATS, starting publisher");
    
    publish_checks(nats, redis_conn, &config)
        .await
        .context("Publisher task failed")?;

    Ok(())
} 
use crate::config::{Config, PREFIXES};
use crate::models::ServerData;
use crate::redis_store::RedisStore;
use anyhow::{Context, Result};
use async_nats::Client as NatsClient;
use chrono::Utc;
use redis::aio::MultiplexedConnection;
use tracing::{debug, error, info};

pub struct Publisher {
    nats: NatsClient,
    redis: RedisStore,
    config: Config,
}

impl Publisher {
    pub fn new(nats: NatsClient, redis: MultiplexedConnection, config: Config) -> Self {
        Self {
            nats,
            redis: RedisStore::new(redis),
            config,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let mut handles = Vec::new();
        
        for &prefix in PREFIXES {
            let nats = self.nats.clone();
            let redis = self.redis.clone();
            let config = self.config.clone();
            let handle = tokio::spawn(Self::publish_checks_for_chain(
                nats,
                redis,
                config,
                prefix,
            ));
            handles.push(handle);
        }

        futures::future::try_join_all(handles)
            .await
            .context("Publisher task failed")?;
        
        Ok(())
    }

    async fn publish_checks_for_chain(
        nats: NatsClient,
        redis: RedisStore,
        config: Config,
        prefix: &'static str,
    ) -> Result<()> {
        let network = prefix.trim_end_matches(':');
        let interval = config.get_interval_for_network(network);
        let mut interval_timer = tokio::time::interval(interval);
        
        tracing::info!("Starting checks for {} with interval {:?}", network, interval);
        
        loop {
            interval_timer.tick().await;
            tracing::debug!("Timer tick for network {}", network);
            
            let keys = match redis.get_keys(prefix).await {
                Ok(keys) => keys,
                Err(e) => {
                    error!("Failed to get Redis keys for prefix {}: {}", prefix, e);
                    continue;
                }
            };

            if keys.is_empty() {
                info!("No keys found for prefix {prefix}");
                continue;
            }

            for key in keys {
                Self::process_key(&nats, &redis, &config, &key, network).await?;
            }
        }
    }

    async fn process_key(
        nats: &NatsClient,
        redis: &RedisStore,
        config: &Config,
        key: &str,
        network: &str,
    ) -> Result<()> {
        let raw_data = match redis.get_server_data(key).await {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to get Redis data for key {}: {}", key, e);
                return Ok(());
            }
        };

        let data: ServerData = match serde_json::from_str(&raw_data) {
            Ok(data) => data,
            Err(e) => {
                error!(
                    "Invalid JSON for key {}: {}\nRaw data: {:?}",
                    key,
                    e,
                    raw_data
                );
                return Ok(());
            }
        };

        if !Self::is_stale(&data, network, config) {
            debug!(%key, "Skipping recently checked server");
            return Ok(());
        }

        let host = key.split_once(':')
            .map(|(_, host)| host)
            .unwrap_or(key);

        let port = data.port.unwrap_or_else(|| Self::default_port_for_network(network));

        let subject = format!("{}check.{}", config.nats_prefix, network);
        let message = Self::create_check_message(network, host, port, &data);

        nats.publish(subject.clone(), message.to_string().into())
            .await
            .context("Failed to publish message")?;

        info!(%key, %subject, "Published check request");
        Ok(())
    }

    fn is_stale(data: &ServerData, network: &str, config: &Config) -> bool {
        let last_updated = match data.last_updated {
            Some(updated) => updated,
            None => return true,
        };

        let now = Utc::now();
        let age = now.signed_duration_since(last_updated);
        let age_secs = age.num_seconds() as u64;

        if age_secs < 30 {
            debug!("Skipping recently checked server ({}s ago)", age_secs);
            return false;
        }

        let interval = config.get_interval_for_network(network);
        let interval_secs = interval.as_secs();

        debug!(
            "Age: {}s, Interval: {}s, Network: {}", 
            age_secs, 
            interval_secs, 
            network
        );

        age_secs >= interval_secs
    }

    fn default_port_for_network(network: &str) -> u16 {
        match network {
            "btc" => 50002,
            "zec" => 9067,
            _ => unreachable!("Unknown network: {network}"),
        }
    }
    fn create_check_message(network: &str, host: &str, port: u16, data: &ServerData) -> serde_json::Value {
        let mut json = serde_json::json!({
            "type": network,
            "host": host,
            "port": port,
            "user_submitted": data.user_submitted,
            "check_id": data.check_id
        });

        if network == "btc" {
            if let Some(obj) = json.as_object_mut() {
                obj.insert(
                    "version".to_string(),
                    serde_json::Value::String(
                        data.version
                            .as_deref()
                            .or(data.electrum_version.as_deref())
                            .unwrap_or("unknown")
                            .to_string()
                    )
                );
            }
        }

        json
    }
} 
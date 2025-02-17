use std::collections::HashMap;
use std::env;
use std::time::Duration;
use anyhow::Result;

const DEFAULT_REFRESH_INTERVAL: u64 = 300;
const DEFAULT_NATS_PREFIX: &str = "hosh.";
const DEFAULT_REDIS_PORT: u16 = 6379;
pub const PREFIXES: &[&str] = &["btc:", "zec:", "http:"];

#[derive(Debug, Clone)]
pub struct Config {
    pub refresh_interval: Duration,
    pub chain_intervals: HashMap<String, Duration>,
    pub nats_url: String,
    pub nats_prefix: String,
    pub redis_host: String,
    pub redis_port: u16,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let default_interval = Duration::from_secs(
            env::var("CHECK_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_REFRESH_INTERVAL),
        );

        let mut chain_intervals = HashMap::new();
        
        for chain in PREFIXES {
            let chain_name = chain.trim_end_matches(':').to_uppercase();
            let env_var = format!("CHECK_INTERVAL_{}", chain_name);
            
            if let Ok(interval_str) = env::var(&env_var) {
                if let Ok(secs) = interval_str.parse::<u64>() {
                    chain_intervals.insert(
                        chain_name.to_lowercase(),
                        Duration::from_secs(secs)
                    );
                }
            }
        }

        let config = Self {
            refresh_interval: default_interval,
            chain_intervals,
            nats_url: env::var("NATS_URL").unwrap_or_else(|_| "nats://nats:4222".into()),
            nats_prefix: env::var("NATS_PREFIX").unwrap_or_else(|_| DEFAULT_NATS_PREFIX.into()),
            redis_host: env::var("REDIS_HOST").unwrap_or_else(|_| "redis".into()),
            redis_port: env::var("REDIS_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_REDIS_PORT),
        };

        tracing::info!("Loaded configuration: default_interval={:?}, chain_intervals={:?}", 
            config.refresh_interval, 
            config.chain_intervals
        );

        Ok(config)
    }

    pub fn get_interval_for_network(&self, network: &str) -> Duration {
        self.chain_intervals
            .get(network)
            .copied()
            .unwrap_or(self.refresh_interval)
    }
} 
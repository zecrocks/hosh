use std::env;
use std::time::Duration;
use anyhow::Result;

const DEFAULT_REFRESH_INTERVAL: u64 = 300;
const DEFAULT_NATS_PREFIX: &str = "hosh.";
pub const PREFIXES: &[&str] = &["btc:", "zec:", "http:"];

#[derive(Debug, Clone)]
pub struct Config {
    pub check_interval: u64,
    pub nats_url: String,
    pub nats_prefix: String,
    pub clickhouse_url: String,
    pub clickhouse_db: String,
    pub clickhouse_user: String,
    pub clickhouse_password: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let check_interval = env::var("CHECK_INTERVAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_REFRESH_INTERVAL);

        Ok(Self {
            check_interval,
            nats_url: env::var("NATS_URL").unwrap_or_else(|_| "nats://nats:4222".into()),
            nats_prefix: env::var("NATS_PREFIX").unwrap_or_else(|_| DEFAULT_NATS_PREFIX.into()),
            clickhouse_url: format!(
                "http://{}:{}",
                env::var("CLICKHOUSE_HOST").unwrap_or_else(|_| "chronicler".into()),
                env::var("CLICKHOUSE_PORT").unwrap_or_else(|_| "8123".into())
            ),
            clickhouse_db: env::var("CLICKHOUSE_DB").unwrap_or_else(|_| "hosh".into()),
            clickhouse_user: env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "hosh".into()),
            clickhouse_password: env::var("CLICKHOUSE_PASSWORD").expect("CLICKHOUSE_PASSWORD environment variable must be set"),
        })
    }

    #[allow(unused_variables)]
    pub fn get_interval_for_network(&self, network: &str) -> Duration {
        // For now, just use the default check_interval for all networks
        Duration::from_secs(self.check_interval)
    }
} 
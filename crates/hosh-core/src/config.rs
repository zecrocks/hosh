//! Unified configuration management for all Hosh services.

use std::env;

/// Configuration for ClickHouse database connection.
#[derive(Clone, Debug)]
pub struct ClickHouseConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

impl ClickHouseConfig {
    /// Create configuration from environment variables.
    pub fn from_env() -> Self {
        Self {
            host: env::var("CLICKHOUSE_HOST").unwrap_or_else(|_| "chronicler".into()),
            port: env::var("CLICKHOUSE_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8123),
            user: env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "hosh".into()),
            password: env::var("CLICKHOUSE_PASSWORD")
                .expect("CLICKHOUSE_PASSWORD environment variable must be set"),
            database: env::var("CLICKHOUSE_DB").unwrap_or_else(|_| "hosh".into()),
        }
    }

    /// Get the HTTP URL for ClickHouse.
    pub fn url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

/// Configuration for checker workers.
#[derive(Clone, Debug)]
pub struct WorkerConfig {
    pub web_api_url: String,
    pub api_key: String,
    pub socks_proxy: Option<String>,
    pub max_concurrent_checks: usize,
}

impl WorkerConfig {
    /// Create configuration from environment variables.
    pub fn from_env() -> Self {
        Self {
            web_api_url: env::var("WEB_API_URL").unwrap_or_else(|_| "http://web:8080".into()),
            api_key: env::var("API_KEY").expect("API_KEY environment variable must be set"),
            socks_proxy: env::var("SOCKS_PROXY").ok(),
            max_concurrent_checks: env::var("MAX_CONCURRENT_CHECKS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
        }
    }
}

/// Configuration for the web service.
#[derive(Clone, Debug)]
pub struct WebConfig {
    pub api_key: String,
    pub results_window_days: u32,
    pub bind_address: String,
    pub bind_port: u16,
}

impl WebConfig {
    /// Create configuration from environment variables.
    pub fn from_env() -> Self {
        Self {
            api_key: env::var("API_KEY").expect("API_KEY environment variable must be set"),
            results_window_days: env::var("RESULTS_WINDOW_DAYS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            bind_address: env::var("BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0".into()),
            bind_port: env::var("BIND_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8080),
        }
    }
}

/// Configuration for the discovery service.
#[derive(Clone, Debug)]
pub struct DiscoveryConfig {
    pub discovery_interval_secs: u64,
}

impl DiscoveryConfig {
    /// Create configuration from environment variables.
    pub fn from_env() -> Self {
        Self {
            discovery_interval_secs: env::var("DISCOVERY_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3600),
        }
    }
}

/// Combined configuration for all services.
#[derive(Clone, Debug)]
pub struct Config {
    pub clickhouse: ClickHouseConfig,
    pub worker: Option<WorkerConfig>,
    pub web: Option<WebConfig>,
    pub discovery: Option<DiscoveryConfig>,
}

impl Config {
    /// Create full configuration from environment variables.
    /// Only initializes sub-configs that don't require mandatory env vars.
    pub fn from_env() -> Self {
        Self {
            clickhouse: ClickHouseConfig::from_env(),
            worker: env::var("API_KEY").ok().map(|_| WorkerConfig::from_env()),
            web: env::var("API_KEY").ok().map(|_| WebConfig::from_env()),
            discovery: Some(DiscoveryConfig::from_env()),
        }
    }
}

//! ClickHouse database client for Hosh.

use crate::config::ClickHouseConfig;
use tracing::{error, info};

/// A client for interacting with ClickHouse.
#[derive(Clone)]
pub struct ClickHouseClient {
    config: ClickHouseConfig,
    http_client: reqwest::Client,
}

impl ClickHouseClient {
    /// Create a new ClickHouse client from configuration.
    pub fn new(config: ClickHouseConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(300))
            .pool_max_idle_per_host(32)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            http_client,
        }
    }

    /// Create a new ClickHouse client from environment variables.
    pub fn from_env() -> Self {
        Self::new(ClickHouseConfig::from_env())
    }

    /// Get the database name.
    pub fn database(&self) -> &str {
        &self.config.database
    }

    /// Execute a query and return the result as a string.
    pub async fn execute_query(
        &self,
        query: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        info!("Executing ClickHouse query");

        let response = self
            .http_client
            .post(&self.config.url())
            .basic_auth(&self.config.user, Some(&self.config.password))
            .header("Content-Type", "text/plain")
            .body(query.to_string())
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await?;
            error!(
                "ClickHouse query failed with status {}: {}",
                status, error_text
            );
            return Err(format!("ClickHouse query failed: {}", error_text).into());
        }

        let result = response.text().await?;
        Ok(result)
    }

    /// Check if a target exists in the targets table.
    pub async fn target_exists(
        &self,
        module: &str,
        hostname: &str,
        port: u16,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let query = format!(
            "SELECT count() FROM {}.targets WHERE module = '{}' AND hostname = '{}' AND port = {}",
            self.config.database, module, hostname, port
        );
        let result = self.execute_query(&query).await?;
        Ok(result.trim().parse::<i64>()? > 0)
    }

    /// Insert a target into the targets table.
    pub async fn insert_target(
        &self,
        module: &str,
        hostname: &str,
        port: u16,
        community: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.target_exists(module, hostname, port).await? {
            info!("Target already exists: {} {}:{}", module, hostname, port);
            return Ok(());
        }

        let query = format!(
            "INSERT INTO TABLE {}.targets (target_id, module, hostname, port, last_queued_at, last_checked_at, user_submitted, community) VALUES (generateUUIDv4(), '{}', '{}', {}, now64(3, 'UTC'), now64(3, 'UTC'), false, {})",
            self.config.database, module, hostname, port, community
        );
        self.execute_query(&query).await?;
        info!(
            "Successfully inserted target: {} {}:{} (community: {})",
            module, hostname, port, community
        );
        Ok(())
    }
}

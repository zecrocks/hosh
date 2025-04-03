use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use anyhow::{Result, Context};
use crate::clickhouse::Target;
use serde_json::json;

/// Legacy Redis store implementation.
/// Kept for reference but no longer actively used.
#[allow(dead_code)]
#[derive(Clone)]
pub struct RedisStore {
    conn: MultiplexedConnection,
}

#[allow(dead_code)]
impl RedisStore {
    pub fn new(conn: MultiplexedConnection) -> Self {
        Self { conn }
    }

    pub async fn get_keys(&self, prefix: &str) -> Result<Vec<String>> {
        self.conn.clone()
            .keys(format!("{}*", prefix))
            .await
            .context("Failed to get Redis keys")
    }

    pub async fn cache_target_info(&self, target: &Target) -> Result<()> {
        let key = format!("{}:{}", target.module, target.hostname);
        let value = json!({
            "hostname": target.hostname,
            "module": target.module,
            "last_queued_at": target.last_queued_at,
            "last_checked_at": target.last_checked_at,
            "user_submitted": target.user_submitted,
            "host": target.hostname,
            "port": match target.module.as_str() {
                "btc" => 50002,
                "zec" => 9067,
                "http" => 80,
                _ => 0,
            },
            "status": "unknown",
            "height": 0,
            "ping": 0.0,
            "error": null
        });

        let _: () = redis::cmd("SET")
            .arg(&key)
            .arg(value.to_string())
            .arg("EX")
            .arg(3600) // 1 hour TTL
            .query_async(&mut self.conn.clone())
            .await?;

        Ok(())
    }

    pub async fn get_server_data(&self, key: &str) -> Result<String> {
        let data: String = redis::cmd("GET")
            .arg(key)
            .query_async(&mut self.conn.clone())
            .await?;
        Ok(data)
    }
} 
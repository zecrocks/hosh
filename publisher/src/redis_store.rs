use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use anyhow::{Result, Context};

#[derive(Clone)]
pub struct RedisStore {
    conn: MultiplexedConnection,
}

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

    pub async fn get_server_data(&self, key: &str) -> Result<String> {
        self.conn.clone()
            .get(key)
            .await
            .context("Failed to get Redis data")
    }
} 
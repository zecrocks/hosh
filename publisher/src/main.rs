use anyhow::{Context, Result};
use publisher::{Config, Publisher};

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

    let publisher = Publisher::new(nats, redis_conn, config);

    publisher.run().await
} 
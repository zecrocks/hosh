use anyhow::{Context, Result};
use futures::StreamExt;
use publisher::{Config, Publisher};
use tracing::{info, error};
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    // Get NATS configuration
    let nats_url = env::var("NATS_URL").map_err(|e| {
        error!("Failed to get NATS_URL: {}", e);
        e
    })?;
    let nats_user = env::var("NATS_USERNAME").unwrap_or_default();
    let nats_password = env::var("NATS_PASSWORD").unwrap_or_default();

    let config = Config::from_env()?;

    let redis_client = redis::Client::open(
        format!("redis://{}:{}", config.redis_host, config.redis_port)
    )?;
    
    let redis_conn = redis_client.get_multiplexed_async_connection()
        .await
        .context("Failed to connect to Redis")?;

    // Create NATS client with credentials
    info!("Attempting NATS connection...");
    let nats = if !nats_user.is_empty() && !nats_password.is_empty() {
        info!("Connecting to NATS with authentication...");
        let client = async_nats::ConnectOptions::new()
            .user_and_password(nats_user, nats_password)
            .connect(&nats_url)
            .await?;
        info!("✅ Successfully authenticated with NATS");
        client
    } else {
        info!("Connecting to NATS without authentication...");
        let client = async_nats::connect(&nats_url).await?;
        info!("✅ Successfully connected to NATS");
        client
    };

    // Verify connection by publishing and receiving a test message
    let nats_subject = env::var("NATS_SUBJECT").unwrap_or_else(|_| "hosh.publisher".to_string());
    let test_subject = format!("{}.test", nats_subject);
    let test_payload = "connection_test";
    
    let mut sub = nats.subscribe(test_subject.clone()).await?;
    nats.publish(test_subject, test_payload.into()).await?;
    
    // Test the connection with timeout
    let timeout_duration = std::time::Duration::from_secs(5);
    match tokio::time::timeout(timeout_duration, sub.next()).await {
        Ok(Some(msg)) => {
            if msg.payload == test_payload.as_bytes() {
                info!("✅ NATS connection verified with test message");
            } else {
                error!("⚠️ NATS test message received but payload mismatch");
            }
        },
        Ok(None) => error!("⚠️ NATS subscription closed unexpectedly"),
        Err(_) => error!("⚠️ NATS test message timeout - connection may be unstable"),
    }

    // Cleanup test subscription
    drop(sub);

    let publisher = Publisher::new(nats, redis_conn, config);

    publisher.run().await
} 
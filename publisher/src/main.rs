use anyhow::{Context, Result};
use publisher::{Config, Publisher};

fn setup_logging() {
    use tracing_subscriber::fmt;
    
    // Initialize tracing subscriber with environment filter
    let subscriber = fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env());
    subscriber.init();
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    let config = Config::from_env()?;

    let nats = async_nats::connect(&config.nats_url)
        .await
        .context("Failed to connect to NATS")?;

    let publisher = Publisher::new(nats, config);

    publisher.run().await
} 
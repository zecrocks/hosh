use anyhow::{Context, Result};
use publisher::{Config, Publisher};

fn setup_logging() {
    use tracing_subscriber::{EnvFilter, fmt};
    use tracing_subscriber::filter::LevelFilter;

    // Create a more restrictive filter
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .parse_lossy("info,hyper=off,reqwest=off,h2=off,tower=off,tonic=off");

    fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false)
        .init();
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
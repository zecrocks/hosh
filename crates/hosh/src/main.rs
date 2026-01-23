//! Hosh - Light wallet server uptime monitoring system.
//!
//! This is the main binary that can run different roles of the Hosh system.

use clap::Parser;
use std::collections::HashSet;
use std::env;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "hosh")]
#[command(author = "Hosh Team")]
#[command(version)]
#[command(about = "Light wallet server uptime monitoring system", long_about = None)]
struct Cli {
    /// Comma-separated list of roles to run: web, checker-btc, checker-zec, discovery, or all
    /// Examples: --roles web  |  --roles all  |  --roles web,discovery,checker-zec
    #[arg(long, default_value = "all")]
    roles: String,

    /// Geographic location identifier for this checker instance (IATA airport code)
    /// Used for multi-region monitoring (e.g., jfk, lax, fra, sin, dxb)
    /// Can also be set via CHECKER_LOCATION env var (CLI flag takes precedence)
    #[arg(long)]
    location: Option<String>,
}

const VALID_ROLES: &[&str] = &["web", "checker-btc", "checker-zec", "discovery", "all"];

/// A future that never completes (used as placeholder in select! when a role is disabled)
async fn pending_forever() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    std::future::pending::<()>().await;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize tracing
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    let cli = Cli::parse();

    // Parse comma-separated roles into a set
    let mut roles: HashSet<String> = cli
        .roles
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Validate roles
    for role in &roles {
        if !VALID_ROLES.contains(&role.as_str()) {
            eprintln!("Unknown role: {}", role);
            eprintln!("Valid roles: web, checker-btc, checker-zec, discovery, all");
            std::process::exit(1);
        }
    }

    // If "all" is specified, expand to all individual roles
    if roles.contains("all") {
        roles.remove("all");
        roles.insert("web".to_string());
        roles.insert("checker-btc".to_string());
        roles.insert("checker-zec".to_string());
        roles.insert("discovery".to_string());
    }

    if roles.is_empty() {
        eprintln!("No roles specified");
        std::process::exit(1);
    }

    info!("Starting Hosh with roles: {:?}", roles);

    // Resolve location: CLI flag > env var > default
    let location = cli
        .location
        .or_else(|| env::var("CHECKER_LOCATION").ok())
        .unwrap_or_else(|| "iah".to_string());
    info!("Checker location: {}", location);

    let run_web = roles.contains("web");
    let run_btc = roles.contains("checker-btc");
    let run_zec = roles.contains("checker-zec");
    let run_discovery = roles.contains("discovery");

    if run_web {
        info!("Starting web server...");
    }
    if run_btc {
        info!("Starting BTC checker...");
    }
    if run_zec {
        info!("Starting ZEC checker...");
    }
    if run_discovery {
        info!("Starting discovery service...");
    }

    // Use tokio::select! to run all enabled roles concurrently
    // Each branch will only be active if the role is enabled
    let btc_location = location.clone();
    let zec_location = location.clone();
    tokio::select! {
        result = async { hosh_web::run().await }, if run_web => {
            match result {
                Ok(()) => info!("Web server completed"),
                Err(e) => error!("Web server error: {}", e),
            }
        }
        result = async { hosh_checker_btc::run_with_location(&btc_location).await }, if run_btc => {
            match result {
                Ok(()) => info!("BTC checker completed"),
                Err(e) => error!("BTC checker error: {}", e),
            }
        }
        result = async { hosh_checker_zec::run_with_location(&zec_location).await }, if run_zec => {
            match result {
                Ok(()) => info!("ZEC checker completed"),
                Err(e) => error!("ZEC checker error: {}", e),
            }
        }
        result = async { hosh_discovery::run().await }, if run_discovery => {
            match result {
                Ok(()) => info!("Discovery service completed"),
                Err(e) => error!("Discovery service error: {}", e),
            }
        }
        // Fallback that never triggers - ensures select! compiles when all conditions are false
        _ = pending_forever(), if !run_web && !run_btc && !run_zec && !run_discovery => {
            unreachable!("No roles were enabled");
        }
    }

    Ok(())
}

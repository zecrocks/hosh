use std::{env, error::Error};
use chrono::{DateTime, Utc};
use redis::Commands;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use zingolib;
use futures_util::StreamExt;
use tracing::{info, error};

#[derive(Debug, Deserialize)]
struct CheckRequest {
    host: String,
    port: u16,
    check_id: Option<String>,
    user_submitted: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CheckResult {
    host: String,
    port: u16,
    height: u64,
    status: String,
    error: Option<String>,
    #[serde(rename = "LastUpdated")]
    last_updated: DateTime<Utc>,
    ping: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    check_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_submitted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vendor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    git_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sapling_activation_height: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    consensus_branch_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    taddr_support: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    build_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    build_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    estimated_height: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    zcashd_build: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    zcashd_subversion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    donation_address: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let nats_prefix = env::var("NATS_PREFIX").unwrap_or_else(|_| "hosh.".into());
    let nats_url = format!(
        "nats://{}:{}",
        env::var("NATS_HOST").unwrap_or_else(|_| "nats".into()),
        env::var("NATS_PORT").unwrap_or_else(|_| "4222".into())
    );

    let redis_url = format!(
        "redis://{}:{}",
        env::var("REDIS_HOST").unwrap_or_else(|_| "redis".into()),
        env::var("REDIS_PORT").unwrap_or_else(|_| "6379".into())
    );

    let mut redis_conn = redis::Client::open(redis_url.as_str())?.get_connection()?;
    info!("Connected to Redis at {}", redis_url);
    
    let nc = async_nats::connect(&nats_url).await?;
    info!("Connected to NATS at {}", nats_url);
    
    let mut sub = nc.subscribe(format!("{}check.zec", nats_prefix)).await?;
    info!("Subscribed to {}check.zec", nats_prefix);

    while let Some(msg) = sub.next().await {
        let check_request: CheckRequest = match serde_json::from_slice(&msg.payload) {
            Ok(req) => req,
            Err(e) => {
                error!("Failed to parse check request: {e}");
                continue;
            }
        };

        let uri = match format!("https://{}:{}", check_request.host, check_request.port).parse() {
            Ok(u) => u,
            Err(e) => {
                error!("Invalid URI: {e}");
                continue;
            }
        };

        let start_time = Instant::now();
        let (height, error, server_info) = match zingolib::grpc_connector::get_info(uri).await {
            Ok(info) => (info.block_height, None, Some(info)),
            Err(e) => (0, Some(e.to_string()), None),
        };

        let latency = start_time.elapsed().as_secs_f64() * 1000.0;
        let ping = (latency * 100.0).round() / 100.0;
        let status = if error.is_none() { "success" } else { "error" };

        match &error {
            Some(err) => info!(
                "Server {}:{} - Error checking block height, Latency: {:.2}ms, Error: {}",
                check_request.host, check_request.port, ping, err
            ),
            None => info!(
                "Server {}:{} - Block height: {}, Latency: {:.2}ms",
                check_request.host, check_request.port, height, ping
            ),
        }

        let result = CheckResult {
            host: check_request.host.clone(),
            port: check_request.port,
            height,
            status: status.into(),
            error,
            last_updated: Utc::now(),
            ping,
            check_id: check_request.check_id,
            user_submitted: check_request.user_submitted,
            vendor: server_info.as_ref().map(|info| info.vendor.clone()),
            git_commit: server_info.as_ref().map(|info| info.git_commit.clone()),
            chain_name: server_info.as_ref().map(|info| info.chain_name.clone()),
            sapling_activation_height: server_info.as_ref().map(|info| info.sapling_activation_height),
            consensus_branch_id: server_info.as_ref().map(|info| info.consensus_branch_id.clone()),
            taddr_support: server_info.as_ref().map(|info| info.taddr_support),
            branch: server_info.as_ref().map(|info| info.branch.clone()),
            build_date: server_info.as_ref().map(|info| info.build_date.clone()),
            build_user: server_info.as_ref().map(|info| info.build_user.clone()),
            estimated_height: server_info.as_ref().map(|info| info.estimated_height),
            server_version: server_info.as_ref().map(|info| info.version.clone()),
            zcashd_build: server_info.as_ref().map(|info| info.zcashd_build.clone()),
            zcashd_subversion: server_info.as_ref().map(|info| info.zcashd_subversion.clone()),
            donation_address: server_info.as_ref().map(|info| info.donation_address.clone()),
        };

        if let Ok(result_json) = serde_json::to_string(&result) {
            let redis_key = format!("zec:{}", check_request.host);
            if let Err(e) = redis_conn.set::<_, _, ()>(&redis_key, &result_json) {
                error!("Redis save failed: {e}");
            }
        }
    }

    Ok(())
}

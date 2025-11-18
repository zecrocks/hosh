use std::{env, error::Error, time::Duration};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use zcash_client_backend::{
    proto::service::{compact_tx_streamer_client::CompactTxStreamerClient, Empty},
};
use tonic::{Request, transport::{Uri, ClientTlsConfig, Endpoint}};
use futures_util::StreamExt;
use tracing::{info, error};
use uuid::Uuid;
use reqwest;

#[derive(Debug, Deserialize)]
struct CheckRequest {
    host: String,
    port: u16,
    check_id: Option<String>,
    user_submitted: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CheckResult {
    checker_module: String,
    hostname: String,
    host: String,
    port: u16,
    height: u64,
    status: String,
    error: Option<String>,
    last_updated: DateTime<Utc>,
    ping: f64,
    ping_ms: f64,
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

#[derive(Debug)]
struct ServerInfo {
    block_height: u64,
    vendor: String,
    git_commit: String,
    chain_name: String,
    sapling_activation_height: u64,
    consensus_branch_id: String,
    taddr_support: bool,
    branch: String,
    build_date: String,
    build_user: String,
    estimated_height: u64,
    version: String,
    zcashd_build: String,
    zcashd_subversion: String,
    donation_address: String,
}

struct ClickhouseConfig {
    url: String,
    user: String,
    password: String,
    database: String,
}

impl ClickhouseConfig {
    fn from_env() -> Self {
        Self {
            url: format!("http://{}:{}", 
                env::var("CLICKHOUSE_HOST").unwrap_or_else(|_| "chronicler".into()),
                env::var("CLICKHOUSE_PORT").unwrap_or_else(|_| "8123".into())
            ),
            user: env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "hosh".into()),
            password: env::var("CLICKHOUSE_PASSWORD").expect("CLICKHOUSE_PASSWORD environment variable must be set"),
            database: env::var("CLICKHOUSE_DB").unwrap_or_else(|_| "hosh".into()),
        }
    }
}

#[derive(Clone)]
struct Worker {
    web_api_url: String,
    api_key: String,
    http_client: reqwest::Client,
}

// Connect directly (without Tor)
async fn get_info_direct(uri: Uri) -> Result<ServerInfo, Box<dyn Error>> {
    info!("Connecting to lightwalletd server at {}", uri);
    
    let endpoint = Endpoint::from(uri.clone())
        .tls_config(ClientTlsConfig::new().with_webpki_roots())?
        .connect_timeout(Duration::from_secs(5))   // dial timeout
        .timeout(Duration::from_secs(15));         // per-RPC client-side timeout
    
    info!("Establishing secure connection...");
    let channel = endpoint.connect().await?;
    
    let mut client = CompactTxStreamerClient::with_origin(channel, uri);
    
    info!("Sending gRPC request for lightwalletd info...");
    let mut req = Request::new(Empty {});
    req.set_timeout(Duration::from_secs(10));  // per-call deadline
    
    let chain_info = match client.get_lightd_info(req).await {
        Ok(response) => {
            info!("Received successful gRPC response");
            response.into_inner()
        },
        Err(e) => {
            error!("gRPC request failed: {}", e);
            return Err(format!("gRPC error: {}", e).into());
        }
    };

    info!("Processing server response...");
    let info = ServerInfo {
        block_height: chain_info.block_height,
        vendor: chain_info.vendor,
        chain_name: chain_info.chain_name,
        git_commit: chain_info.git_commit,
        sapling_activation_height: chain_info.sapling_activation_height,
        consensus_branch_id: chain_info.consensus_branch_id,
        taddr_support: chain_info.taddr_support,
        branch: chain_info.branch,
        build_date: chain_info.build_date,
        build_user: chain_info.build_user,
        estimated_height: chain_info.estimated_height,
        version: chain_info.version,
        zcashd_build: chain_info.zcashd_build,
        zcashd_subversion: chain_info.zcashd_subversion,
        donation_address: chain_info.donation_address,
    };

    info!("Successfully gathered server info");
    Ok(info)
}

impl Worker {
    pub async fn new() -> Result<Self, Box<dyn Error>> {
        rustls::crypto::ring::default_provider()
            .install_default()
            .expect("Failed to install rustls crypto provider");

        let web_api_url = env::var("WEB_API_URL").unwrap_or_else(|_| "http://web:8080".to_string());
        let api_key = env::var("API_KEY").expect("API_KEY environment variable must be set");

        info!("🚀 Initializing ZEC Worker with web API URL: {}", web_api_url);
        
        let http_client = reqwest::Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(300))
            .pool_max_idle_per_host(32)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .build()?;

        Ok(Worker {
            web_api_url,
            api_key,
            http_client,
        })
    }

    async fn submit_to_api(&self, result: &CheckResult) -> Result<(), Box<dyn Error>> {
        let response = self.http_client.post(&format!("{}/api/v1/results?api_key={}", self.web_api_url, self.api_key))
            .json(result)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("API submission error: {}", response.text().await?).into());
        }

        info!(
            host = %result.host,
            check_id = %result.check_id.as_deref().unwrap_or("none"),
            "Successfully submitted check data to API"
        );

        Ok(())
    }

    async fn process_check(&self, check_request: CheckRequest) -> Result<(), Box<dyn Error>> {
        let uri: Uri = match format!("https://{}:{}", check_request.host, check_request.port).parse() {
            Ok(u) => u,
            Err(e) => {
                error!("Invalid URI: {e}");
                return Ok(());
            }
        };

        let start_time = Instant::now();

        let (height, error, server_info) = match get_info_direct(uri).await {
            Ok(info) => (info.block_height, None, Some(info)),
            Err(e) => {
                let simplified_error = if e.to_string().contains("tls handshake eof") {
                    "TLS handshake failed - server may be offline or not accepting connections".to_string()
                } else if e.to_string().contains("connection refused") {
                    "Connection refused - server may be offline or not accepting connections".to_string()
                } else if e.to_string().contains("InvalidContentType") {
                    "Invalid content type - server may not be a valid Zcash node".to_string()
                } else {
                    let error_str = e.to_string();
                    if let Some(start) = error_str.find("message: \"") {
                        let start = start + 10;
                        if let Some(end) = error_str[start..].find("\", source:") {
                            error_str[start..start + end].to_string()
                        } else if let Some(end) = error_str[start..].find("\"") {
                            error_str[start..start + end].to_string()
                        } else {
                            error_str
                        }
                    } else {
                        error_str
                    }
                };
                (0, Some(simplified_error), None)
            },
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
            checker_module: "zec".to_string(),
            hostname: check_request.host.clone(),
            host: check_request.host.clone(),
            port: check_request.port,
            height,
            status: if error.is_none() { "online".to_string() } else { "offline".to_string() },
            error,
            last_updated: Utc::now(),
            ping,
            ping_ms: ping,
            check_id: check_request.check_id.clone(),
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

        if let Err(e) = self.submit_to_api(&result).await {
            error!(%e, "Failed to publish data to API");
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    
    let subscriber = tracing_subscriber::fmt();
    if args.len() > 1 && args[1] == "--test" {
        subscriber.with_max_level(tracing::Level::INFO).init();
        info!("Running in test mode");
    } else {
        subscriber.init();
    }
    
    if args.len() > 1 && args[1] == "--test" {
        let target = if args.len() > 2 {
            args[2].clone()
        } else {
            "zec.rocks:443".to_string()
        };
        
        let parts: Vec<&str> = target.split(':').collect();
        let (test_server, test_port) = if parts.len() >= 2 {
            (parts[0].to_string(), parts[1].parse().unwrap_or(443))
        } else {
            (target.clone(), 443)
        };
        
        // Check if it's an onion address which we don't support yet
        if test_server.ends_with(".onion") {
            error!("Cannot connect to .onion address: Tor support is disabled");
            return Err("Cannot connect to .onion addresses: Tor support is disabled".into());
        }
        
        info!("Testing direct connection to {}:{}", test_server, test_port);
        
        let uri_str = format!("https://{}:{}", test_server, test_port);
        info!("Constructing URI: {}", uri_str);
        
        let uri = match uri_str.parse::<Uri>() {
            Ok(u) => u,
            Err(e) => {
                error!("Failed to parse URI: {}", e);
                return Err(format!("URI parsing error: {}", e).into());
            }
        };
        
        let wait_time = 20;
        
        info!("Starting connection attempt to {} (timeout: {} seconds)...", uri, wait_time);
        let start_time = Instant::now();
        
        match tokio::time::timeout(
            Duration::from_secs(wait_time),
            get_info_direct(uri)
        ).await {
            Ok(result) => {
                match result {
                    Ok(info) => {
                        let latency = start_time.elapsed().as_secs_f64() * 1000.0;
                        info!("Successfully connected! Block height: {}, Latency: {:.2}ms", 
                             info.block_height, latency);
                    },
                    Err(e) => {
                        error!("Failed to connect: {}", e);
                    }
                }
            },
            Err(_) => {
                error!("Connection timed out after {} seconds", wait_time);
            }
        }
        
        return Ok(());
    }
    
    info!("Starting ZEC checker in normal mode");
    let worker = Worker::new().await?;
    
    loop {
        info!("📡 Fetching jobs from web API...");
        let jobs_url = format!("{}/api/v1/jobs?api_key={}&checker_module=zec&limit=10", worker.web_api_url, worker.api_key);
        match worker.http_client.get(&jobs_url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<Vec<CheckRequest>>().await {
                        Ok(jobs) => {
                            info!("✅ Found {} jobs", jobs.len());
                            let mut handles = Vec::new();
                            for job in jobs {
                                let worker_clone = worker.clone();
                                handles.push(tokio::spawn(async move {
                                    if let Err(e) = worker_clone.process_check(job).await {
                                        error!("Error processing check: {}", e);
                                    }
                                }));
                            }
                            futures_util::future::join_all(handles).await;
                        }
                        Err(e) => {
                            error!("❌ Failed to parse jobs from web API: {}", e);
                        }
                    }
                } else {
                    error!("❌ Web API returned non-success status: {}", response.status());
                }
            }
            Err(e) => {
                error!("❌ Failed to fetch jobs from web API: {}", e);
            }
        }
        
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

use std::{env, error::Error, time::Duration};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use zcash_client_backend::{
    proto::service::{compact_tx_streamer_client::CompactTxStreamerClient, Empty},
};
use tonic::{Request, transport::{Uri, ClientTlsConfig, Endpoint}};
use tracing::{info, error};

mod socks_connector;
use socks_connector::SocksConnector;

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

// ClickhouseConfig removed - not used in current implementation
// Results are submitted via Worker's submit_to_api method instead

#[derive(Clone)]
struct Worker {
    web_api_url: String,
    api_key: String,
    http_client: reqwest::Client,
}

// Connect directly (without SOCKS proxy)
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

// Connect via SOCKS5 proxy (e.g., Tor)
async fn get_info_via_socks(uri: Uri, proxy_addr: String) -> Result<ServerInfo, Box<dyn Error>> {
    info!("Connecting to lightwalletd server at {} via SOCKS proxy {}", uri, proxy_addr);
    
    let connector = SocksConnector::new(proxy_addr);
    
    // Check if this is an .onion address
    let host = uri.host().unwrap_or("");
    let is_onion = host.ends_with(".onion");
    
    // Many .onion services run without TLS since Tor already provides encryption
    // Try to construct the URI with http:// scheme for .onion addresses
    let connection_uri = if is_onion {
        info!("Detected .onion address - using plaintext connection (Tor provides encryption)");
        // Construct http:// URI for plaintext gRPC
        format!("http://{}:{}", host, uri.port_u16().unwrap_or(443))
            .parse::<Uri>()?
    } else {
        uri.clone()
    };
    
    // Use longer timeouts for SOCKS connections (Tor circuit building can be slow)
    let mut endpoint = Endpoint::from(connection_uri.clone())
        .connect_timeout(Duration::from_secs(10))  // Longer timeout for SOCKS
        .timeout(Duration::from_secs(20));         // Longer RPC timeout
    
    // Only configure TLS for non-.onion addresses
    if !is_onion {
        endpoint = endpoint.tls_config(ClientTlsConfig::new().with_webpki_roots())?;
    }
    
    info!("Establishing connection through SOCKS proxy...");
    let channel = endpoint
        .connect_with_connector(connector)
        .await?;
    
    let mut client = CompactTxStreamerClient::with_origin(channel, connection_uri);
    
    info!("Sending gRPC request for lightwalletd info...");
    let mut req = Request::new(Empty {});
    req.set_timeout(Duration::from_secs(10));
    
    let chain_info = match client.get_lightd_info(req).await {
        Ok(response) => {
            info!("Received successful gRPC response via SOCKS");
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

    info!("Successfully gathered server info via SOCKS");
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
        let response = self.http_client.post(format!("{}/api/v1/results?api_key={}", self.web_api_url, self.api_key))
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

        // Check if this is an .onion address
        let is_onion = check_request.host.ends_with(".onion");
        let socks_proxy = env::var("SOCKS_PROXY").ok();

        let (height, error, server_info) = if is_onion {
            // .onion addresses require SOCKS proxy
            if let Some(proxy) = socks_proxy {
                info!("Using SOCKS proxy for .onion address: {}", proxy);
                match get_info_via_socks(uri, proxy).await {
                    Ok(info) => (info.block_height, None, Some(info)),
                    Err(e) => {
                        error!("SOCKS connection failed: {}", e);
                        (0, Some(e.to_string()), None)
                    },
                }
            } else {
                error!(".onion address requires SOCKS_PROXY to be configured");
                (0, Some("Cannot connect to .onion address without SOCKS proxy".to_string()), None)
            }
        } else {
            // Use direct connection for regular addresses
            info!("Using direct connection");
            match get_info_direct(uri).await {
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
            }
        };

        // Only calculate meaningful ping for successful connections
        // For failed .onion connections, don't record the failure time as "ping"
        let (ping, ping_ms) = if error.is_none() {
            let latency = start_time.elapsed().as_secs_f64() * 1000.0;
            let ping_value = (latency * 100.0).round() / 100.0;
            (ping_value, ping_value)
        } else if is_onion {
            // Don't record ping for failed .onion connections
            (0.0, 0.0)
        } else {
            // For regular servers, record the failure time (useful for detecting timeouts)
            let latency = start_time.elapsed().as_secs_f64() * 1000.0;
            let ping_value = (latency * 100.0).round() / 100.0;
            (ping_value, ping_value)
        };

        match &error {
            Some(err) => {
                if is_onion {
                    info!(
                        "Server {}:{} (.onion) - Connection failed, Error: {}",
                        check_request.host, check_request.port, err
                    );
                } else {
                    info!(
                        "Server {}:{} - Error checking block height, Latency: {:.2}ms, Error: {}",
                        check_request.host, check_request.port, ping, err
                    );
                }
            },
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
            ping_ms,
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
        
        // Check if this is an .onion address
        let is_onion = test_server.ends_with(".onion");
        let socks_proxy = env::var("SOCKS_PROXY").ok();
        
        // .onion addresses require SOCKS proxy
        if is_onion && socks_proxy.is_none() {
            error!("Cannot connect to .onion address without SOCKS proxy");
            error!("Set SOCKS_PROXY environment variable (e.g., SOCKS_PROXY=127.0.0.1:9050)");
            return Err("Cannot connect to .onion addresses without SOCKS proxy".into());
        }
        
        if is_onion {
            if let Some(ref proxy) = socks_proxy {
                info!("Testing SOCKS connection via {} to .onion address {}:{}", proxy, test_server, test_port);
            }
        } else {
            info!("Testing direct connection to {}:{}", test_server, test_port);
        }
        
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
        
        // Only use SOCKS for .onion addresses
        let connection_result = if is_onion {
            if let Some(proxy) = socks_proxy {
                tokio::time::timeout(
                    Duration::from_secs(wait_time),
                    get_info_via_socks(uri, proxy)
                ).await
            } else {
                // This shouldn't happen due to earlier check, but handle it
                return Err("Cannot connect to .onion address without SOCKS proxy".into());
            }
        } else {
            tokio::time::timeout(
                Duration::from_secs(wait_time),
                get_info_direct(uri)
            ).await
        };
        
        match connection_result {
            Ok(result) => {
                match result {
                    Ok(info) => {
                        let latency = start_time.elapsed().as_secs_f64() * 1000.0;
                        info!("Successfully connected! Block height: {}, Latency: {:.2}ms", 
                             info.block_height, latency);
                        info!("Server details: vendor={}, version={}, chain={}", 
                             info.vendor, info.version, info.chain_name);
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

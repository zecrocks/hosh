use std::env;
use std::error::Error;
use std::fmt;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::{self, Uuid};
use reqwest;
use tracing::{error, info};
use std::process::Command;
use tracing_subscriber;
use std::collections::HashMap;
use crate::types::BlockchainInfo;

mod blockchair;
mod blockchaindotcom;
mod blockstream;
mod mempool;
mod zecrocks;
mod zcashexplorer;
mod types;


#[derive(Debug, Deserialize)]
struct CheckRequest {
    #[serde(default)]
    url: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default)]
    check_id: Option<String>,
    #[serde(default)]
    user_submitted: Option<bool>,
    #[serde(default)]
    dry_run: bool,
}

fn default_port() -> u16 { 80 }

#[allow(dead_code)]
#[derive(Debug, Serialize)]
struct CheckResult {
    checker_module: String,
    hostname: String,
    host: String,
    port: u16,
    height: u64,
    status: String,
    error: Option<String>,
    #[serde(rename = "LastUpdated")]
    last_updated: DateTime<Utc>,
    ping: f64,
    ping_ms: f64,
    check_id: Option<String>,
    user_submitted: Option<bool>,
}

#[derive(Clone)]
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

impl Worker {
    async fn new() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let web_api_url = env::var("WEB_API_URL").unwrap_or_else(|_| "http://web:8080".to_string());
        let api_key = env::var("API_KEY").expect("API_KEY environment variable must be set");

        info!("🚀 Initializing HTTP Worker with web API URL: {}", web_api_url);

        let http_client = reqwest::Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(300))
            .pool_max_idle_per_host(32)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        Ok(Worker { 
            web_api_url,
            api_key,
            http_client 
        })
    }

    async fn process_check(&self, check_request: CheckRequest) {
        // Check if dry run mode is enabled
        if check_request.dry_run {
            info!(
                "DRY RUN: Received check request - url={} port={} check_id={} user_submitted={}",
                check_request.url,
                check_request.port,
                check_request.check_id.as_deref().unwrap_or("none"),
                check_request.user_submitted.unwrap_or(false)
            );
            return;
        }

        info!("Starting blockchain info fetching for URL: {}", check_request.url);
        
        // Determine which explorer to check based on URL
        let (explorer_name, explorer_result): (String, Result<HashMap<String, BlockchainInfo>, Box<dyn Error + Send + Sync>>) = if check_request.url.contains("blockchair.com") {
            ("blockchair".to_string(), blockchair::get_blockchain_info().await)
        } else if check_request.url.contains("blkchair") && check_request.url.contains(".onion") {
            ("blockchair-onion".to_string(), blockchair::get_onion_blockchain_info(&check_request.url).await)
        } else if check_request.url.contains("blockstream.info") {
            ("blockstream".to_string(), blockstream::get_blockchain_info().await)
        } else if check_request.url.contains("zec.rocks") {
            ("zecrocks".to_string(), zecrocks::get_blockchain_info().await)
        } else if check_request.url.contains("blockchain.com") {
            ("blockchain".to_string(), blockchaindotcom::get_blockchain_info().await)
        } else if check_request.url.contains("zcashexplorer.app") {
            ("zcashexplorer".to_string(), zcashexplorer::get_blockchain_info().await)
        } else if check_request.url.contains("mempool.space") {
            ("mempool".to_string(), mempool::get_blockchain_info().await)
        } else {
            error!("Unknown explorer URL: {}", check_request.url);
            return;
        };

        match explorer_result {
            Ok(data) => {
                for (chain_id, info) in data.iter() {
                    if let Some(height) = info.height {
                        let result = CheckResult {
                            checker_module: "http".to_string(),
                            hostname: format!("{}.{}", chain_id, chain_id),
                            host: format!("{}.{}", chain_id, chain_id),
                            port: check_request.port,
                            height,
                            status: "online".to_string(),
                            error: None,
                            last_updated: Utc::now(),
                            ping: info.response_time_ms as f64,
                            ping_ms: info.response_time_ms as f64,
                            check_id: check_request.check_id.clone(),
                            user_submitted: check_request.user_submitted,
                        };
                        
                        if let Err(e) = self.submit_to_api(&explorer_name, chain_id, &result).await {
                            error!("Failed to publish to API for {}.{}: {}", explorer_name, chain_id, e);
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to fetch blockchain info: {}", e);
            }
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        loop {
            info!("📡 Fetching jobs from web API...");
            let jobs_url = format!("{}/api/v1/jobs?api_key={}&checker_module=http&limit=10", self.web_api_url, self.api_key);
            match self.http_client.get(&jobs_url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<Vec<CheckRequest>>().await {
                            Ok(jobs) => {
                                info!("✅ Found {} jobs", jobs.len());
                                let mut handles = Vec::new();
                                for job in jobs {
                                    let worker_clone = self.clone();
                                    handles.push(tokio::spawn(async move {
                                        worker_clone.process_check(job).await;
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
            
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    }

    async fn submit_to_api(&self, source: &str, chain_id: &str, result: &CheckResult) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!("📊 Publishing to API for {}.{}", source, chain_id);

        let response = self.http_client.post(&format!("{}/api/v1/results?api_key={}", self.web_api_url, self.api_key))
            .json(result)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("❌ API submission failed: {}", error_text);
            return Err(format!("API submission error: {}", error_text).into());
        }
        info!("✅ API submission successful");

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("info,html5ever=error")
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false)
        .with_ansi(true)
        .init();

    // Clear screen based on platform
    if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "cls"])
            .status()
            .expect("Failed to clear screen");
    } else {
        Command::new("clear")
            .status()
            .expect("Failed to clear screen");
    }

    info!("🔍 HTTP Block Explorer Checker Starting...");
    info!("==========================================\n");

    info!("Testing Tor connection...");
    if let Ok(_client) = blockchair::blockchairdotonion::create_client() {
        info!("✅ Successfully created Tor client");
    } else {
        error!("❌ Failed to create Tor client");
    }

    // Add ClickHouse connection test
    let worker = Worker::new().await?;
    
    worker.run().await
}
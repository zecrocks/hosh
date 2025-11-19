use serde::{Deserialize, Serialize};
use std::env;
use futures_util::stream::StreamExt;
use crate::routes::electrum::query::{electrum_query, QueryParams};
use axum::extract::Query;
use tracing::{debug, error, info};

#[derive(Debug, Serialize, Deserialize)]
struct CheckRequest {
    #[serde(default)]
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_version")]
    version: String,
    #[serde(default)]
    check_id: Option<String>,
    #[serde(default)]
    user_submitted: bool,
}

fn default_port() -> u16 { 50002 }
fn default_version() -> String { "unknown".to_string() }

#[derive(Debug, Serialize)]
struct ServerData {
    checker_module: String,
    hostname: String,
    host: String,
    port: u16,
    height: u64,
    #[serde(rename = "server_version")]
    electrum_version: String,
    last_updated: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ping: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ping_ms: Option<f64>,
    error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: Option<String>,
    user_submitted: bool,
    check_id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    additional_data: Option<serde_json::Value>,
}

#[derive(Clone)]
pub struct Worker {
    web_api_url: String,
    api_key: String,
    max_concurrent_checks: usize,
    http_client: reqwest::Client,
}

impl Worker {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let web_api_url = env::var("WEB_API_URL").unwrap_or_else(|_| "http://web:8080".to_string());
        let api_key = env::var("API_KEY").expect("API_KEY environment variable must be set");
        
        info!("🚀 Initializing BTC Worker with web API URL: {}", web_api_url);
        
        let max_concurrent_checks = env::var("MAX_CONCURRENT_CHECKS")
            .unwrap_or_else(|_| "3".to_string())
            .parse()
            .unwrap_or(10);
            
        info!("⚙️ Setting max concurrent checks to: {}", max_concurrent_checks);
            
        // Create a pooled HTTP client
        info!("🌐 Creating HTTP client with connection pooling...");
        let http_client = reqwest::Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(300))
            .pool_max_idle_per_host(32)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .build()?;
        info!("✅ HTTP client created successfully");

        Ok(Worker {
            web_api_url,
            api_key,
            max_concurrent_checks,
            http_client,
        })
    }

    async fn query_server_data(&self, request: &CheckRequest) -> Option<ServerData> {
        info!("🔍 Querying server data for {}:{}", request.host, request.port);
        let params = QueryParams {
            url: request.host.clone(),
            port: Some(request.port),
        };

        match electrum_query(Query(params)).await {
            Ok(response) => {
                info!("✅ Successfully queried server {}:{}", request.host, request.port);
                let data = response.0;
                let filtered_data = serde_json::json!({
                    "bits": data["bits"],
                    "connection_type": data["connection_type"],
                    "merkle_root": data["merkle_root"],
                    "method_used": data["method_used"],
                    "nonce": data["nonce"],
                    "prev_block": data["prev_block"],
                    "resolved_ips": data["resolved_ips"],
                    "self_signed": data["self_signed"],
                    "server_version": data["server_version"],
                    "timestamp": data["timestamp"],
                    "timestamp_human": data["timestamp_human"],
                    "tls_version": data["tls_version"],
                    "version": data["version"]
                });

                let height = data["height"].as_u64().unwrap_or(0);
                info!("📊 Server {}:{} - Block height: {}", request.host, request.port, height);

                Some(ServerData {
                    checker_module: "btc".to_string(),
                    hostname: request.host.clone(),
                    host: request.host.clone(),
                    port: request.port,
                    height,
                    electrum_version: data.get("server_version")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&request.version)
                        .to_string(),
                    last_updated: chrono::Utc::now(),
                    ping: data.get("ping").and_then(|v| v.as_f64()),
                    ping_ms: data.get("ping").and_then(|v| v.as_f64()),
                    error: false,
                    error_type: None,
                    error_message: None,
                    user_submitted: request.user_submitted,
                    check_id: request.get_check_id(),
                    status: "online".to_string(),
                    additional_data: Some(filtered_data),
                })
            }
            Err(e) => {
                // Extract error message in a serializable format
                let error_message = format!("Failed to query server: {:?}", e);
                error!("❌ Failed to query server {}:{} - {}", request.host, request.port, error_message);
                
                Some(ServerData {
                    checker_module: "btc".to_string(),
                    hostname: request.host.clone(),
                    host: request.host.clone(),
                    port: request.port,
                    height: 0,
                    electrum_version: request.version.clone(),
                    last_updated: chrono::Utc::now(),
                    ping: None,
                    ping_ms: None,
                    error: true,
                    error_type: Some("connection_error".to_string()),
                    error_message: Some(error_message),
                    user_submitted: request.user_submitted,
                    check_id: request.get_check_id(),
                    status: "offline".to_string(),
                    additional_data: None,
                })
            }
        }
    }

    async fn submit_check_data(&self, server_data: &ServerData) -> Result<(), Box<dyn std::error::Error>> {
        info!("💾 Submitting check data for {}:{}", server_data.host, server_data.port);

        let response = self.http_client.post(format!("{}/api/v1/results?api_key={}", self.web_api_url, self.api_key))
            .json(server_data)
            .send()
            .await?;

        if !response.status().is_success() {
            error!("❌ API submission error: {}", response.text().await?);
            return Err("API submission failed".into());
        }
        
        info!("✅ Successfully submitted check result to web API");
        
        Ok(())
    }

    async fn process_check_request(&self, request: CheckRequest) {
        debug!("Processing check request: {:?}", request);
        
        info!(
            host = %request.host,
            check_id = %request.get_check_id(),
            user_submitted = %request.user_submitted,
            "Processing check request"
        );

        if let Some(server_data) = self.query_server_data(&request).await {
            // Store data in ClickHouse
            if let Err(e) = self.submit_check_data(&server_data).await {
                error!(%e, "Failed to submit data to web API");
            }
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            max_concurrent = %self.max_concurrent_checks,
            "🚀 Starting BTC checker worker"
        );
        
        let (tx, mut rx) = tokio::sync::mpsc::channel(self.max_concurrent_checks);
        let worker = self.clone();
        
        let _process_handle = tokio::spawn(async move {
            let mut handles = futures_util::stream::FuturesUnordered::new();
            
            while let Some(req) = rx.recv().await {
                if handles.len() >= worker.max_concurrent_checks {
                    info!("⏳ Waiting for a slot to become available...");
                    handles.next().await;
                }
                
                let worker = worker.clone();
                handles.push(tokio::spawn(async move {
                    worker.process_check_request(req).await;
                }));
            }
            
            while let Some(result) = handles.next().await {
                if let Err(e) = result {
                    error!("❌ Task error: {}", e);
                }
            }
        });

        loop {
            info!("📡 Fetching jobs from web API...");
            let jobs_url = format!("{}/api/v1/jobs?api_key={}&checker_module=btc&limit={}", self.web_api_url, self.api_key, self.max_concurrent_checks);
            match self.http_client.get(&jobs_url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<Vec<CheckRequest>>().await {
                            Ok(jobs) => {
                                info!("✅ Found {} jobs", jobs.len());
                                for job in jobs {
                                    if let Err(e) = tx.send(job).await {
                                        error!("❌ Failed to queue message: {}", e);
                                    }
                                }
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
            
            // Wait for 10 seconds before polling again
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    }
}

impl CheckRequest {
    fn get_check_id(&self) -> String {
        self.check_id.clone().unwrap_or_else(|| "none".to_string())
    }
} 
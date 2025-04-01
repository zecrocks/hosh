use async_nats::Client as NatsClient;
use serde::{Deserialize, Serialize};
use std::env;
use futures_util::stream::StreamExt;
use crate::routes::electrum::query::{electrum_query, QueryParams};
use axum::extract::Query;
use tracing::{debug, error, info};
use uuid::Uuid;
use reqwest;
use uuid;

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
    host: String,
    port: u16,
    height: u64,
    #[serde(rename = "server_version")]
    electrum_version: String,
    last_updated: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ping: Option<f64>,
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
    nats: NatsClient,
    clickhouse_url: String,
    clickhouse_user: String,
    clickhouse_password: String,
    clickhouse_db: String,
    nats_subject: String,
    max_concurrent_checks: usize,
    http_client: reqwest::Client,
}

impl Worker {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let nats_url = env::var("NATS_URL").unwrap_or_else(|_| "nats://nats:4222".to_string());
        let nats_subject = env::var("NATS_SUBJECT").unwrap_or_else(|_| "hosh.check.btc".to_string());
        
        info!("🚀 Initializing BTC Worker with NATS URL: {}", nats_url);
        
        let max_concurrent_checks = env::var("MAX_CONCURRENT_CHECKS")
            .unwrap_or_else(|_| "3".to_string())
            .parse()
            .unwrap_or(10);
            
        // ClickHouse configuration
        let clickhouse_host = env::var("CLICKHOUSE_HOST").unwrap_or_else(|_| "chronicler".to_string());
        let clickhouse_port = env::var("CLICKHOUSE_PORT").unwrap_or_else(|_| "8123".to_string()); // HTTP port
        let clickhouse_db = env::var("CLICKHOUSE_DB").unwrap_or_else(|_| "hosh".to_string());
        let clickhouse_user = env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "hosh".to_string());
        let clickhouse_password = env::var("CLICKHOUSE_PASSWORD").unwrap_or_else(|_| "chron".to_string());

        // Create ClickHouse URL
        let clickhouse_url = format!("http://{}:{}", clickhouse_host, clickhouse_port);

        let nats = async_nats::connect(&nats_url).await?;
        
        // Create a pooled HTTP client
        let http_client = reqwest::Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(300))
            .pool_max_idle_per_host(32)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .build()?;

        Ok(Worker {
            nats,
            clickhouse_url,
            clickhouse_user,
            clickhouse_password,
            clickhouse_db,
            nats_subject,
            max_concurrent_checks,
            http_client,
        })
    }

    async fn query_server_data(&self, request: &CheckRequest) -> Option<ServerData> {
        let params = QueryParams {
            url: request.host.clone(),
            port: Some(request.port),
        };

        match electrum_query(Query(params)).await {
            Ok(response) => {
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

                Some(ServerData {
                    host: request.host.clone(),
                    port: request.port,
                    height: data["height"].as_u64().unwrap_or(0),
                    electrum_version: data.get("server_version")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&request.version)
                        .to_string(),
                    last_updated: chrono::Utc::now(),
                    ping: data.get("ping").and_then(|v| v.as_f64()),
                    error: false,
                    error_type: None,
                    error_message: None,
                    user_submitted: request.user_submitted,
                    check_id: request.get_check_id(),
                    status: "success".to_string(),
                    additional_data: Some(filtered_data),
                })
            }
            Err(error_response) => {
                Some(ServerData {
                    host: request.host.clone(),
                    port: request.port,
                    height: 0,
                    electrum_version: request.version.clone(),
                    last_updated: chrono::Utc::now(),
                    ping: None,
                    error: true,
                    error_type: Some("connection_error".to_string()),
                    error_message: Some(format!("Failed to query server: {:?}", error_response)),
                    user_submitted: request.user_submitted,
                    check_id: request.get_check_id(),
                    status: "error".to_string(),
                    additional_data: None,
                })
            }
        }
    }

    async fn store_check_data(&self, request: &CheckRequest, server_data: &ServerData) -> Result<(), Box<dyn std::error::Error>> {
        let escaped_host = request.host.replace("'", "\\'");
        
        // Generate a deterministic UUID v5 using DNS namespace and hostname
        let target_id = uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_DNS,
            format!("btc:{}", request.host).as_bytes()
        ).to_string();

        // Update existing target or create new one if doesn't exist
        let upsert_query = format!(
            "INSERT INTO {db}.targets (target_id, module, hostname, last_queued_at, last_checked_at, user_submitted)
             SELECT '{target_id}', 'btc', '{host}', now(), now(), {user_submitted}
             WHERE NOT EXISTS (
                 SELECT 1 FROM {db}.targets 
                 WHERE module = 'btc' AND hostname = '{host}'
             )",
            db = self.clickhouse_db,
            target_id = target_id,
            host = escaped_host,
            user_submitted = request.user_submitted
        );

        let response = self.http_client.post(&self.clickhouse_url)
            .basic_auth(&self.clickhouse_user, Some(&self.clickhouse_password))
            .header("Content-Type", "text/plain")
            .body(upsert_query)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("ClickHouse insert error: {}", response.text().await?).into());
        }

        // Then update the target's timestamps and ensure target_id is consistent
        let update_query = format!(
            "ALTER TABLE {db}.targets 
             UPDATE last_queued_at = now(),
                    last_checked_at = now(),
                    target_id = '{target_id}'
             WHERE module = 'btc' AND hostname = '{host}'
             SETTINGS mutations_sync = 1",
            db = self.clickhouse_db,
            target_id = target_id,
            host = escaped_host,
        );

        // Execute update query
        let response = self.http_client.post(&self.clickhouse_url)
            .basic_auth(&self.clickhouse_user, Some(&self.clickhouse_password))
            .header("Content-Type", "text/plain")
            .body(update_query)
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(format!("ClickHouse update error: {}", response.text().await?).into());
        }

        // Prepare resolved IP (if available)
        let resolved_ip = match &server_data.additional_data {
            Some(data) => {
                if let Some(ips) = data.get("resolved_ips") {
                    if let Some(ip_array) = ips.as_array() {
                        if !ip_array.is_empty() {
                            if let Some(first_ip) = ip_array[0].as_str() {
                                first_ip.to_string()
                            } else {
                                "".to_string()
                            }
                        } else {
                            "".to_string()
                        }
                    } else {
                        "".to_string()
                    }
                } else {
                    "".to_string()
                }
            },
            None => "".to_string(),
        };
        
        // Determine status
        let status = if server_data.error {
            "offline"
        } else {
            "online"
        };
        
        // Insert the result
        let result_query = format!(
            "INSERT INTO {}.results 
             (target_id, checked_at, hostname, resolved_ip, ip_version, 
              checker_module, status, ping_ms, checker_location, checker_id, response_data, user_submitted) 
             VALUES 
             ('{}', now(), '{}', '{}', 4, 'btc', '{}', {}, 'default', '{}', '{}', {})",
            self.clickhouse_db,
            target_id,
            server_data.host.replace("'", "\\'"),
            resolved_ip.replace("'", "\\'"),
            status,
            server_data.ping.unwrap_or(0.0),
            Uuid::new_v4(), // Generate a checker_id
            serde_json::to_string(server_data)?.replace("'", "\\'"),
            request.user_submitted
        );
        
        let response = self.http_client.post(&format!("{}", self.clickhouse_url))
            .basic_auth(&self.clickhouse_user, Some(&self.clickhouse_password))
            .header("Content-Type", "text/plain")
            .body(result_query)
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(format!("ClickHouse error: {}", response.text().await?).into());
        }
        
        info!(
            host = %request.host,
            check_id = %request.get_check_id(),
            "Successfully saved check data to ClickHouse"
        );
        
        Ok(())
    }

    async fn process_check_request(&self, msg: async_nats::Message) {
        debug!("Raw message payload: {:?}", String::from_utf8_lossy(&msg.payload));

        let data = match String::from_utf8(msg.payload.to_vec()) {
            Ok(data) => {
                debug!("Parsed UTF-8 string: {}", data);
                data
            }
            Err(e) => {
                error!("Failed to parse message payload as UTF-8: {}", e);
                return;
            }
        };

        // Try to parse the JSON directly, in case there are formatting issues
        match serde_json::from_str::<serde_json::Value>(&data) {
            Ok(value) => {
                debug!("Parsed as generic JSON value: {:?}", value);
                
                // Extract fields manually
                let host = value.get("host")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                    
                let hostname = value.get("hostname")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                    
                // Use hostname as fallback if host is empty
                let final_host = if host.is_empty() { hostname } else { host };
                
                if final_host.is_empty() {
                    error!("Both host and hostname are empty in message: {}", data);
                    return;
                }
                
                let port = value.get("port")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(50002) as u16;
                    
                let version = value.get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                    
                let check_id = value.get("check_id")
                    .and_then(|v| v.as_str())
                    .or_else(|| value.get("target_id").and_then(|v| v.as_str()))
                    .map(|s| s.to_string());
                    
                let user_submitted = value.get("user_submitted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                    
                let request = CheckRequest {
                    host: final_host,
                    port,
                    version,
                    check_id,
                    user_submitted,
                };
                
                debug!("Manually constructed CheckRequest: {:?}", request);
                
                info!(
                    host = %request.host,
                    check_id = %request.get_check_id(),
                    user_submitted = %request.user_submitted,
                    "Processing check request"
                );

                if let Some(server_data) = self.query_server_data(&request).await {
                    // Store data in ClickHouse
                    if let Err(e) = self.store_check_data(&request, &server_data).await {
                        error!(%e, "Failed to publish data to ClickHouse");
                    }
                }
            },
            Err(e) => {
                error!("Failed to parse JSON: {} - Data: {}", e, data);
                return;
            }
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            subject = %self.nats_subject,
            max_concurrent = %self.max_concurrent_checks,
            "Starting BTC checker worker"
        );

        let mut subscriber = self.nats.subscribe(self.nats_subject.clone()).await?;
        
        // Create a channel with bounded capacity for concurrent processing
        let (tx, mut rx) = tokio::sync::mpsc::channel(self.max_concurrent_checks);
        
        // Clone necessary data for the spawned task
        let worker = self.clone();
        
        // Spawn task to process messages from channel
        let process_handle = tokio::spawn(async move {
            let mut handles = futures_util::stream::FuturesUnordered::new();
            
            while let Some(msg) = rx.recv().await {
                if handles.len() >= worker.max_concurrent_checks {
                    handles.next().await;
                }
                
                let worker = worker.clone();
                handles.push(tokio::spawn(async move {
                    worker.process_check_request(msg).await;
                }));
            }
            
            while let Some(result) = handles.next().await {
                if let Err(e) = result {
                    error!("Task error: {}", e);
                }
            }
        });

        while let Some(msg) = subscriber.next().await {
            if let Err(e) = tx.send(msg).await {
                error!("Failed to queue message: {}", e);
            }
        }

        drop(tx);
        process_handle.await?;

        Ok(())
    }
}

impl CheckRequest {
    fn is_valid(&self) -> bool {
        // If the host field is empty but we have a check_id/target_id, we'll consider it valid
        if self.host.is_empty() {
            return false;
        }
        
        if self.user_submitted {
            self.check_id.is_some() && !self.host.is_empty()
        } else {
            !self.host.is_empty()
        }
    }

    fn get_check_id(&self) -> String {
        self.check_id.clone().unwrap_or_else(|| "none".to_string())
    }
} 
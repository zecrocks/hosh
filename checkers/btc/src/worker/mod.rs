use async_nats::Client as NatsClient;
use redis::Client as RedisClient;
use serde::{Deserialize, Serialize};
use std::env;
use futures_util::stream::StreamExt;
use crate::routes::electrum::query::{electrum_query, QueryParams};
use axum::extract::Query;
use tracing::{debug, error, info};
use uuid::Uuid;
use chrono::Utc;
use reqwest;

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
    redis: RedisClient,
    clickhouse_url: String,
    clickhouse_user: String,
    clickhouse_password: String,
    clickhouse_db: String,
    nats_subject: String,
    max_concurrent_checks: usize,
}

impl Worker {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let nats_url = env::var("NATS_URL").unwrap_or_else(|_| "nats://nats:4222".to_string());
        let nats_subject = env::var("NATS_SUBJECT").unwrap_or_else(|_| "hosh.check.btc".to_string());
        let redis_host = env::var("REDIS_HOST").unwrap_or_else(|_| "redis".to_string());
        let redis_port = env::var("REDIS_PORT").unwrap_or_else(|_| "6379".to_string());
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
        let redis = redis::Client::open(format!("redis://{}:{}", redis_host, redis_port))?;
        
        Ok(Worker {
            nats,
            redis,
            clickhouse_url,
            clickhouse_user,
            clickhouse_password,
            clickhouse_db,
            nats_subject,
            max_concurrent_checks,
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

    async fn publish_to_clickhouse(&self, request: &CheckRequest, server_data: &ServerData) -> Result<(), Box<dyn std::error::Error>> {
        // Generate a UUID for the target if not already present
        let target_id = Uuid::parse_str(&request.get_check_id())
            .unwrap_or_else(|_| Uuid::new_v4());
        
        // First, ensure the target exists in the targets table
        let target_query = format!(
            "INSERT INTO targets (target_id, module, hostname, last_queued_at, last_checked_at, user_submitted) 
             VALUES ('{}', 'btc', '{}', now(), now(), {})",
            target_id, 
            request.host.replace("'", "\\'"), 
            request.user_submitted
        );
        
        // Execute the query using reqwest
        let client = reqwest::Client::new();
        let response = client.post(&format!("{}", self.clickhouse_url))
            .query(&[("database", &self.clickhouse_db)])
            .basic_auth(&self.clickhouse_user, Some(&self.clickhouse_password))
            .header("Content-Type", "text/plain")
            .body(target_query)
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(format!("ClickHouse error: {}", response.text().await?).into());
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
            "INSERT INTO results 
             (target_id, checked_at, hostname, resolved_ip, ip_version, 
              checker_module, status, ping_ms, checker_location, checker_id, response_data) 
             VALUES 
             ('{}', now(), '{}', '{}', 4, 'btc', '{}', {}, 'default', '{}', '{}')",
            target_id,
            server_data.host.replace("'", "\\'"),
            resolved_ip.replace("'", "\\'"),
            status,
            server_data.ping.unwrap_or(0.0),
            Uuid::new_v4(), // Generate a checker_id
            serde_json::to_string(server_data)?.replace("'", "\\'")
        );
        
        let response = client.post(&format!("{}", self.clickhouse_url))
            .query(&[("database", &self.clickhouse_db)])
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

        let request = match serde_json::from_str::<CheckRequest>(&data) {
            Ok(req) => {
                if !req.is_valid() {
                    error!(?req, "Invalid request - missing required fields");
                    return;
                }
                debug!(?req, "Successfully parsed request");
                req
            }
            Err(e) => {
                error!(%e, data = %data, "Failed to parse check request");
                return;
            }
        };

        info!(
            host = %request.host,
            check_id = %request.get_check_id(),
            user_submitted = %request.user_submitted,
            "Processing check request"
        );

        if let Some(server_data) = self.query_server_data(&request).await {
            // First publish to ClickHouse
            if let Err(e) = self.publish_to_clickhouse(&request, &server_data).await {
                error!(%e, "Failed to publish data to ClickHouse");
            }
            
            // Then publish to Redis as before
            let redis_key = format!("btc:{}", request.host);
            let redis_value = serde_json::to_string(&server_data).unwrap();

            let mut redis_conn = match self.redis.get_async_connection().await {
                Ok(conn) => conn,
                Err(e) => {
                    error!(%e, "Failed to connect to Redis");
                    return;
                }
            };

            if let Err(e) = redis::cmd("SET")
                .arg(&redis_key)
                .arg(&redis_value)
                .query_async::<_, ()>(&mut redis_conn)
                .await
            {
                error!(%e, "Failed to save data to Redis");
            } else {
                info!(
                    host = %request.host,
                    check_id = %request.get_check_id(),
                    "Successfully saved check data to Redis"
                );
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
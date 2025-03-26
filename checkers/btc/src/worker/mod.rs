use async_nats::Client as NatsClient;
use redis::Client as RedisClient;
use serde::{Deserialize, Serialize};
use std::env;
use futures_util::stream::StreamExt;
use crate::routes::electrum::query::{electrum_query, QueryParams};
use axum::extract::Query;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

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
    nats_subject: String,
    max_concurrent_checks: usize,
}

impl Worker {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Get NATS configuration
        let nats_url = env::var("NATS_URL").map_err(|e| {
            error!("Failed to get NATS_URL: {}", e);
            e
        })?;
        let nats_user = env::var("NATS_USERNAME").unwrap_or_default();
        let nats_password = env::var("NATS_PASSWORD").unwrap_or_default();
        
        info!("Raw NATS URL from env: {}", nats_url);

        // Create NATS client with credentials
        info!("Attempting NATS connection...");
        let nats = if !nats_user.is_empty() && !nats_password.is_empty() {
            info!("Connecting to NATS with authentication...");
            let client = async_nats::ConnectOptions::new()
                .user_and_password(nats_user, nats_password)
                .connect(&nats_url)
                .await?;
            info!("✅ Successfully authenticated with NATS");
            client
        } else {
            info!("Connecting to NATS without authentication...");
            let client = async_nats::connect(&nats_url).await?;
            info!("✅ Successfully connected to NATS");
            client
        };

        // Verify connection by publishing and receiving a test message
        let nats_subject = env::var("NATS_SUBJECT").unwrap_or_else(|_| "hosh.check.btc".to_string());
        let test_subject = format!("{}.test.{}", nats_subject, Uuid::new_v4());
        let test_payload = "connection_test";
        
        let mut sub = nats.subscribe(test_subject.clone()).await?;
        nats.publish(test_subject, test_payload.into()).await?;
        
        // Test the connection with timeout
        let timeout_duration = std::time::Duration::from_secs(5);
        match tokio::time::timeout(timeout_duration, sub.next()).await {
            Ok(Some(msg)) => {
                if msg.payload == test_payload.as_bytes() {
                    info!("✅ NATS connection verified with test message");
                } else {
                    warn!("⚠️ NATS test message received but payload mismatch");
                }
            },
            Ok(None) => warn!("⚠️ NATS subscription closed unexpectedly"),
            Err(_) => warn!("⚠️ NATS test message timeout - connection may be unstable"),
        }

        // Cleanup test subscription
        drop(sub);

        // Get Redis configuration and connect
        let redis_host = env::var("REDIS_HOST").unwrap_or_else(|_| "redis".to_string());
        let redis_port = env::var("REDIS_PORT").unwrap_or_else(|_| "6379".to_string());
        let redis = redis::Client::open(format!("redis://{}:{}", redis_host, redis_port))?;

        let max_concurrent_checks = env::var("MAX_CONCURRENT_CHECKS")
            .unwrap_or_else(|_| "3".to_string())
            .parse()
            .unwrap_or(10);

        Ok(Worker {
            nats,
            redis,
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

        loop {
            match self.nats.subscribe(self.nats_subject.clone()).await {
                Ok(mut subscriber) => {
                    info!("Successfully subscribed to {}", self.nats_subject);
                    
                    // Create a channel with bounded capacity for concurrent processing
                    let (tx, mut rx) = tokio::sync::mpsc::channel(self.max_concurrent_checks);
                    
                    // Clone necessary data for the spawned task
                    let worker = self.clone();
                    
                    // Spawn task to process messages from channel
                    let _process_handle = tokio::spawn(async move {
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
                    
                    error!("NATS subscription stream ended unexpectedly");
                }
                Err(e) => {
                    error!("Failed to subscribe to NATS: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    continue;
                }
            }
        }
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
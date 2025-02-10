use async_nats::Client as NatsClient;
use redis::Client as RedisClient;
use serde::{Deserialize, Serialize};
use std::env;
use futures_util::stream::StreamExt;
use crate::routes::electrum::query::{electrum_query, QueryParams};
use axum::extract::Query;

#[derive(Debug, Serialize, Deserialize)]
struct CheckRequest {
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_version")]
    version: String,
    #[serde(default = "default_check_id")]
    check_id: String,
    #[serde(default)]
    user_submitted: bool,
}

fn default_port() -> u16 { 50002 }
fn default_version() -> String { "unknown".to_string() }
fn default_check_id() -> String { "none".to_string() }

#[derive(Debug, Serialize)]
struct ServerData {
    host: String,
    port: u16,
    height: u64,
    electrum_version: String,
    last_updated: String,
    error: bool,
    error_type: Option<String>,
    error_message: Option<String>,
    user_submitted: bool,
    check_id: String,
    #[serde(flatten)]
    additional_data: Option<serde_json::Value>,
}

pub struct Worker {
    nats: NatsClient,
    redis: RedisClient,
    nats_subject: String,
}

impl Worker {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let nats_url = env::var("NATS_URL").unwrap_or_else(|_| "nats://nats:4222".to_string());
        let nats_subject = env::var("NATS_SUBJECT").unwrap_or_else(|_| "hosh.check.btc".to_string());
        let redis_host = env::var("REDIS_HOST").unwrap_or_else(|_| "redis".to_string());
        let redis_port = env::var("REDIS_PORT").unwrap_or_else(|_| "6379".to_string());

        let nats = async_nats::connect(&nats_url).await?;
        let redis = redis::Client::open(format!("redis://{}:{}", redis_host, redis_port))?;

        Ok(Worker {
            nats,
            redis,
            nats_subject,
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
                Some(ServerData {
                    host: request.host.clone(),
                    port: request.port,
                    height: data["height"].as_u64().unwrap_or(0),
                    electrum_version: request.version.clone(),
                    last_updated: chrono::Utc::now().to_rfc3339(),
                    error: false,
                    error_type: None,
                    error_message: None,
                    user_submitted: request.user_submitted,
                    check_id: request.check_id.clone(),
                    additional_data: Some(data),
                })
            }
            Err(error_response) => {
                // Parse error response and create error ServerData
                Some(ServerData {
                    host: request.host.clone(),
                    port: request.port,
                    height: 0,
                    electrum_version: request.version.clone(),
                    last_updated: chrono::Utc::now().to_rfc3339(),
                    error: true,
                    error_type: Some("connection_error".to_string()),
                    error_message: Some(format!("Failed to query server: {:?}", error_response)),
                    user_submitted: request.user_submitted,
                    check_id: request.check_id.clone(),
                    additional_data: None,
                })
            }
        }
    }

    async fn process_check_request(&self, msg: async_nats::Message) {
        let data = match String::from_utf8(msg.payload.to_vec()) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to parse message payload: {}", e);
                return;
            }
        };

        let request: CheckRequest = match serde_json::from_str(&data) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Failed to parse check request: {}", e);
                return;
            }
        };

        println!("📥 Received check request - host: {}, check_id: {}, user_submitted: {}", 
                request.host, request.check_id, request.user_submitted);

        if let Some(server_data) = self.query_server_data(&request).await {
            let redis_key = format!("btc:{}", request.host);
            let redis_value = serde_json::to_string(&server_data).unwrap();

            let mut redis_conn = match self.redis.get_async_connection().await {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("Failed to connect to Redis: {}", e);
                    return;
                }
            };

            if let Err(e) = redis::cmd("SET")
                .arg(&redis_key)
                .arg(&redis_value)
                .query_async::<_, ()>(&mut redis_conn)
                .await
            {
                eprintln!("Failed to save data to Redis: {}", e);
            } else {
                println!("✅ Data saved to Redis - host: {}, check_id: {}", 
                        request.host, request.check_id);
            }
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🎯 Subscribing to NATS subject: {}", self.nats_subject);

        let mut subscriber = self.nats.subscribe(self.nats_subject.clone()).await?;
        
        while let Some(msg) = subscriber.next().await {
            self.process_check_request(msg).await;
        }

        Ok(())
    }
} 
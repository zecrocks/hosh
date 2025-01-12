use std::env;
use std::collections::HashMap;
use actix_web::{get, web::{self, Redirect}, App, HttpResponse, HttpServer, Result};
use actix_files as fs;
use askama::Template;
use redis::Commands;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    servers: Vec<ServerInfo>,
    percentile_height: u64,
    current_network: &'static str,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct ServerInfo {
    #[serde(default)]
    host: String,

    #[serde(default, deserialize_with = "deserialize_port")]
    port: Option<u16>,

    #[serde(default)]
    height: u64,

    #[serde(rename = "LastUpdated", default)]
    last_updated: String,

    #[serde(default)]
    ping: Option<f64>,

    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

fn deserialize_port<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Always deserialize as Value first to handle any JSON type
    let value = serde_json::Value::deserialize(deserializer)?;
    
    // Convert the value to a string
    let port_str = match value {
        serde_json::Value::String(s) => s,
        serde_json::Value::Number(n) => n.to_string(),
        _ => return Ok(None),
    };
    
    // Try to parse the string as a number
    port_str.parse::<u16>()
        .map(Some)
        .or_else(|_| Ok(None))
}

impl ServerInfo {
    fn formatted_ping(&self) -> String {
        match self.ping {
            Some(p) => format!("{:.2}ms", p),
            None => "-".to_string(),
        }
    }

    // TODO: Show status based on something other than height
    fn is_online(&self) -> bool {
        self.height > 0
    }

    fn is_height_behind(&self, percentile_height: &u64) -> bool {
        // Consider a server behind if it's more than 3 blocks behind the 90th percentile
        self.height > 0 && self.height + 3 < *percentile_height
    }

    fn host_with_port(&self) -> String {
        if let Some(port) = self.port {
            format!("{}:{}", self.host, port)
        } else {
            self.host.clone()
        }
    }

    fn is_height_ahead(&self, percentile_height: &u64) -> bool {
        // Consider a server suspiciously ahead if it's more than 3 blocks ahead of the 90th percentile
        self.height > 0 && self.height > percentile_height + 3
    }

    fn get_rank(&self, percentile_height: &u64) -> u8 {
        if !self.is_online() {
            0
        } else if self.is_height_behind(percentile_height) {
            1
        } else if self.is_height_ahead(percentile_height) {
            2
        } else {
            3
        }
    }
}

#[derive(Debug)]
struct SafeNetwork(&'static str);

impl SafeNetwork {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "btc" => Some(SafeNetwork("btc")),
            "zec" => Some(SafeNetwork("zec")),
            _ => None
        }
    }
    
    fn redis_prefix(&self) -> String {
        format!("{}:*", self.0)
    }
}

#[derive(Template)]
#[template(path = "server.html")]
struct ServerTemplate {
    data: HashMap<String, Value>,
    host: String,
    network: String,
    current_network: &'static str,
    percentile_height: u64,
}

#[derive(Serialize)]
struct ApiServerInfo {
    hostname: String,
    port: u16,
    protocol: &'static str,
    ping: Option<f64>,
    online: bool,
}

#[derive(Serialize)]
struct ApiResponse {
    servers: Vec<ApiServerInfo>
}

#[get("/")]
async fn root() -> Result<Redirect> {
    Ok(Redirect::to("/zec"))
}

#[get("/{network}")]
async fn network_status(
    redis: web::Data<redis::Client>,
    network: web::Path<String>,
) -> Result<HttpResponse> {
    let network = SafeNetwork::from_str(&network)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid network"))?;

    let mut conn = redis.get_connection().map_err(|e| {
        eprintln!("Redis connection error: {}", e);
        actix_web::error::ErrorInternalServerError("Redis connection failed")
    })?;

    let keys: Vec<String> = conn.keys(network.redis_prefix()).map_err(|e| {
        eprintln!("Redis keys error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to fetch Redis keys")
    })?;

    let mut servers = Vec::new();

    for key in keys {
        let value: String = conn.get(&key).map_err(|e| {
            eprintln!("Redis get error for key {}: {}", key, e);
            actix_web::error::ErrorInternalServerError("Failed to fetch Redis value")
        })?;

        match serde_json::from_str::<ServerInfo>(&value) {
            Ok(server_info) => servers.push(server_info),
            Err(e) => {
                eprintln!("Failed to deserialize JSON for key {}: {}", key, e);
                println!("Retrieved JSON for key {}: {}", key, value);
            }
        }
    }

    // Calculate 90th percentile of block heights (only for online servers) FIRST
    let mut heights: Vec<u64> = servers
        .iter()
        .filter(|s| s.height > 0)
        .map(|s| s.height)
        .collect();
    
    heights.sort_unstable();
    let percentile_height = if !heights.is_empty() {
        let index = (heights.len() as f64 * 0.9).ceil() as usize - 1;
        heights[index.min(heights.len() - 1)]
    } else {
        0
    };

    // THEN sort servers using the calculated percentile_height
    servers.sort_by(|a, b| {
        // First compare by rank
        b.get_rank(&percentile_height).cmp(&a.get_rank(&percentile_height))
            // Then by height in reverse order
            .then_with(|| b.height.cmp(&a.height))
            // Finally by ping
            .then_with(|| {
                a.ping
                    .unwrap_or(f64::MAX)
                    .partial_cmp(&b.ping.unwrap_or(f64::MAX))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let template = IndexTemplate { 
        servers,
        percentile_height,
        current_network: network.0,
    };
    
    let html = template.render().map_err(|e| {
        eprintln!("Template rendering error: {}", e);
        actix_web::error::ErrorInternalServerError("Template rendering failed")
    })?;

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html))
}

#[get("/{network}/{host}")]
async fn server_detail(
    redis: web::Data<redis::Client>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let (network, host) = path.into_inner();
    let safe_network = SafeNetwork::from_str(&network)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid network"))?;
    
    let key = format!("{}:{}", network, host);
    
    let mut conn = redis.get_connection().map_err(|e| {
        eprintln!("Redis connection error: {}", e);
        actix_web::error::ErrorInternalServerError("Redis connection failed")
    })?;
    
    let value: String = conn.get(&key).map_err(|e| {
        eprintln!("Redis get error for key {}: {}", key, e);
        actix_web::error::ErrorInternalServerError("Failed to fetch Redis value")
    })?;
    
    let data: HashMap<String, Value> = serde_json::from_str(&value).map_err(|e| {
        eprintln!("JSON deserialization error for key {}: {}", key, e);
        actix_web::error::ErrorInternalServerError("Failed to parse server data")
    })?;
    
    // Get all servers to calculate percentile height
    let keys: Vec<String> = conn.keys(safe_network.redis_prefix()).map_err(|e| {
        eprintln!("Redis keys error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to fetch Redis keys")
    })?;

    let mut heights = Vec::new();
    for key in keys {
        if let Ok(value) = conn.get::<_, String>(&key) {
            if let Ok(server_info) = serde_json::from_str::<ServerInfo>(&value) {
                if server_info.height > 0 {
                    heights.push(server_info.height);
                }
            }
        }
    }

    heights.sort_unstable();
    let percentile_height = if !heights.is_empty() {
        let index = (heights.len() as f64 * 0.9).ceil() as usize - 1;
        heights[index.min(heights.len() - 1)]
    } else {
        0
    };
    
    let template = ServerTemplate {
        data,
        host,
        network,
        current_network: safe_network.0,
        percentile_height,
    };
    
    let html = template.render().map_err(|e| {
        eprintln!("Template rendering error: {}", e);
        actix_web::error::ErrorInternalServerError("Template rendering failed")
    })?;
    
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html))
}

#[get("/api/v0/{network}.json")]
async fn network_api(
    redis: web::Data<redis::Client>,
    network: web::Path<String>,
) -> Result<HttpResponse> {
    let network = SafeNetwork::from_str(&network)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid network"))?;
    
    let servers = fetch_network_servers(&redis, network.0).await?;
    
    let api_servers: Vec<ApiServerInfo> = servers.into_iter()
        .map(|server| {
            let (port, protocol) = match network.0 {
                "btc" => (server.port.unwrap_or(50002), "ssl"),
                "zec" => (server.port.unwrap_or(9067), "grpc"),
                _ => unreachable!(), // SafeNetwork::from_str ensures this
            };
            
            ApiServerInfo {
                hostname: server.host.clone(),
                port,
                protocol,
                ping: server.ping,
                online: server.is_online(),
            }
        })
        .collect();

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .json(ApiResponse { servers: api_servers }))
}

// Helper function to reduce code duplication
async fn fetch_network_servers(redis: &redis::Client, network: &str) -> Result<Vec<ServerInfo>> {
    let mut conn = redis.get_connection().map_err(|e| {
        eprintln!("Redis connection error: {}", e);
        actix_web::error::ErrorInternalServerError("Redis connection failed")
    })?;

    let keys: Vec<String> = conn.keys(format!("{}:*", network)).map_err(|e| {
        eprintln!("Redis keys error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to fetch Redis keys")
    })?;

    let mut servers = Vec::new();
    for key in keys {
        let value: String = conn.get(&key).map_err(|e| {
            eprintln!("Redis get error for key {}: {}", key, e);
            actix_web::error::ErrorInternalServerError("Failed to fetch Redis value")
        })?;

        match serde_json::from_str::<ServerInfo>(&value) {
            Ok(server_info) => servers.push(server_info),
            Err(e) => {
                eprintln!("Failed to deserialize JSON for key {}: {}", key, e);
                println!("Retrieved JSON for key {}: {}", key, value);
            }
        }
    }

    Ok(servers)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let redis_host = env::var("REDIS_HOST").unwrap_or_else(|_| "redis".to_string());
    let redis_port = env::var("REDIS_PORT").unwrap_or_else(|_| "6379".to_string());
    let redis_url = format!("redis://{}:{}", redis_host, redis_port);

    let redis_client = redis::Client::open(redis_url.as_str())
        .expect("Failed to create Redis client");

    println!("Starting server at http://0.0.0.0:8080");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(redis_client.clone()))
            .service(fs::Files::new("/static", "./static"))
            .service(root)
            .service(network_status)
            .service(server_detail)
            .service(network_api)
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}

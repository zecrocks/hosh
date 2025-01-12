use std::env;
use std::collections::HashMap;
use actix_web::{get, web::{self, Redirect}, App, HttpResponse, HttpServer, Result};
use askama::Template;
use redis::Commands;
use serde::{Deserialize, Serialize};

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

    #[serde(default)]
    height: u64,

    #[serde(rename = "LastUpdated", default)]
    last_updated: String, // Stored as a string

    #[serde(default)]
    ping: Option<f64>,

    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
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

    // Calculate 90th percentile of block heights (only for online servers)
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
            .service(root)
            .service(network_status)
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}

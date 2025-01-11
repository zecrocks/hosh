use std::env;
use std::collections::HashMap;
use actix_web::{get, web, App, HttpResponse, HttpServer, Result};
use askama::Template;
use redis::Commands;
use serde::{Deserialize, Serialize};

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    servers: Vec<ServerInfo>,
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
}

#[get("/")]
async fn index(redis: web::Data<redis::Client>) -> Result<HttpResponse> {
    let mut conn = redis.get_connection().map_err(|e| {
        eprintln!("Redis connection error: {}", e);
        actix_web::error::ErrorInternalServerError("Redis connection failed")
    })?;

    let keys: Vec<String> = conn.keys("*").map_err(|e| {
        eprintln!("Redis keys error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to fetch Redis keys")
    })?;

    println!("Retrieved keys: {:?}", keys);

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

    let template = IndexTemplate { servers };
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
            .service(index)
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}

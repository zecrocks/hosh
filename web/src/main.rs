use std::env;
use std::collections::HashMap;
use actix_web::{get, post, web::{self, Redirect}, App, HttpResponse, HttpServer, Result};
use actix_files as fs;
use askama::Template;
use redis::Commands;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use chrono::{DateTime, Utc, FixedOffset};
use uuid::Uuid;
use rand::Rng;
use std::collections::HashSet;
use tracing::{info, warn, error, Level};
use tracing_subscriber::FmtSubscriber;

mod filters {
    use askama::Result;
    use serde_json::Value;

    #[allow(dead_code)]
    pub fn format_value(v: &Value) -> Result<String> {
        match v {
            Value::String(s) => Ok(s.to_string()),
            Value::Number(n) => Ok(n.to_string()),
            Value::Bool(b) => Ok(b.to_string()),
            Value::Null => Ok("null".to_string()),
            _ => Ok(v.to_string())
        }
    }
}


fn upper(s: &str) -> askama::Result<String> {
    Ok(s.to_uppercase())
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    servers: Vec<ServerInfo>,
    percentile_height: u64,
    current_network: &'static str,
    online_count: usize,
    total_count: usize,
    check_error: Option<&'a str>,
    math_problem: (u8, u8),
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct ServerInfo {
    #[serde(default)]
    host: String,

    #[serde(default, deserialize_with = "deserialize_port")]
    port: Option<u16>,

    #[serde(default)]
    height: u64,

    #[serde(default)]
    last_updated: Option<String>,

    #[serde(default)]
    ping: Option<f64>,

    #[serde(default)]
    server_version: Option<String>,

    #[serde(default)]
    error: Option<bool>,

    #[serde(default)]
    error_time: Option<String>,

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

    fn formatted_last_updated(&self) -> String {
        if let Some(last_updated) = &self.last_updated {
            // Try parsing with DateTime::parse_from_rfc3339 first
            let parsed_time = DateTime::parse_from_rfc3339(last_updated)
                // If that fails, try parsing as a naive datetime and assume UTC
                .or_else(|_| {
                    DateTime::parse_from_rfc3339(&format!("{}Z", last_updated))
                })
                .or_else(|_| {
                    // Parse as naive datetime and convert to UTC
                    chrono::NaiveDateTime::parse_from_str(last_updated, "%Y-%m-%dT%H:%M:%S%.f")
                        .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
                        .map(|dt| dt.with_timezone(&FixedOffset::east_opt(0).unwrap()))
                });

            if let Ok(time) = parsed_time {
                let now = Utc::now().with_timezone(time.offset());
                let duration = now.signed_duration_since(time);

                let total_seconds = duration.num_seconds();
                if total_seconds < 0 {
                    return "Just now".to_string();
                }

                if total_seconds < 60 {
                    return format!("{}s", total_seconds);
                }

                let minutes = total_seconds / 60;
                if minutes < 60 {
                    let seconds = total_seconds % 60;
                    return format!("{}m {}s", minutes, seconds);
                }

                let hours = minutes / 60;
                if hours < 24 {
                    let mins = minutes % 60;
                    return format!("{}h {}m", hours, mins);
                }

                let days = hours / 24;
                let hrs = hours % 24;
                format!("{}d {}h", days, hrs)
            } else {
                format!("Invalid time: {}", last_updated)  // Include the timestamp for debugging
            }
        } else {
            "Never".to_string()
        }
    }

    // TODO: Show status based on something other than height
    fn is_online(&self) -> bool {
        !self.error.unwrap_or(false) && self.height > 0
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

    fn formatted_version(&self) -> String {
        let lwd_version = self.server_version
            .as_ref()
            .map(String::as_str)
            .unwrap_or("-")
            .to_string();

        // Display both LWD and Zebra versions for ZEC if available
        if let Some(subversion) = self.extra.get("zcashd_subversion") {
            if let Some(subversion_str) = subversion.as_str() {
                // Remove slashes from subversion string
                let cleaned_subversion = subversion_str.replace('/', "");
                return format!("{}\nLWD: {}", cleaned_subversion, lwd_version);
            }
        }
        
        lwd_version
    }
}

#[derive(Debug)]
struct SafeNetwork(&'static str);

impl SafeNetwork {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "btc" => Some(SafeNetwork("btc")),
            "zec" => Some(SafeNetwork("zec")),
            "http" => Some(SafeNetwork("http")),
            _ => None
        }
    }
    
    fn redis_prefix(&self) -> String {
        format!("{}:*", self.0)
    }
}

#[derive(Template)]
#[template(path = "server.html", escape = "none")]
struct ServerTemplate {
    data: HashMap<String, Value>,
    host: String,
    network: String,
    current_network: &'static str,
    percentile_height: u64,
    online_count: usize,
    total_count: usize,
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

#[derive(Template)]
#[template(path = "check.html", escape = "none")]
struct CheckTemplate {
    check_id: String,
    server: Option<ServerInfo>,
    network: String,
    is_public_server: bool,
    checking_url: Option<String>,
    checking_port: Option<u16>,
    server_data: Option<HashMap<String, Value>>,
}

impl CheckTemplate {
    fn network_upper(network: &str) -> String {
        network.to_uppercase()
    }
}

#[derive(Deserialize)]
struct CheckQuery {
    host: Option<String>,
    port: Option<u16>,
}

#[derive(Template)]
#[template(path = "blockchain_heights.html")]
struct BlockchainHeightsTemplate {
    rows: Vec<ExplorerRow>,
}

impl BlockchainHeightsTemplate {
    // Helper function to format chain names
    fn format_chain_name(&self, chain: &str) -> String {
        chain.replace("-", " ")
    }

    fn get_all_symbols(&self) -> Vec<String> {
        let mut symbols = HashSet::new();
        
        // Collect all unique chain names from all sources
        for row in &self.rows {
            symbols.insert(row.chain.clone());
        }
        
        // Sort them for consistent display
        let mut symbols: Vec<_> = symbols.into_iter().collect();
        symbols.sort_unstable();
        symbols
    }

    fn get_height_difference(&self, height: &u64, row: &ExplorerRow) -> Option<String> {
        // Collect all heights for this row
        let mut heights: Vec<u64> = vec![];
        if let Some(h) = row.blockchair { heights.push(h); }
        if let Some(h) = row.blockchain_com { heights.push(h); }
        if let Some(h) = row.blockstream { heights.push(h); }
        if let Some(h) = row.zecrocks { heights.push(h); }
        if let Some(h) = row.zcashexplorer { heights.push(h); }

        // If we have at least 2 heights (to compare), and this height exists
        if heights.len() >= 2 {
            let min_height = heights.iter().copied().min()?;
            let diff = (*height).saturating_sub(min_height);
            if diff > 0 {
                return Some(format!(" (+{})", diff));
            }
        }
        None
    }
}

#[derive(Debug)]
struct ExplorerRow {
    chain: String,
    blockchair: Option<u64>,
    blockchain_com: Option<u64>,
    blockstream: Option<u64>,
    zecrocks: Option<u64>,
    zcashexplorer: Option<u64>,
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
        error!("Redis connection error: {}", e);
        actix_web::error::ErrorInternalServerError("Redis connection failed")
    })?;

    let keys: Vec<String> = conn.keys(network.redis_prefix()).map_err(|e| {
        error!("Redis keys error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to fetch Redis keys")
    })?;

    let mut servers = Vec::new();

    for key in keys {
        let value: String = conn.get(&key).map_err(|e| {
            error!("Redis get error for key {}: {}", key, e);
            actix_web::error::ErrorInternalServerError("Failed to fetch Redis value")
        })?;

        match serde_json::from_str::<ServerInfo>(&value) {
            Ok(mut server_info) => {
                // Skip servers without last_updated
                if server_info.last_updated.is_none() {
                    continue;
                }
                // Check if `last_updated` is the default value and convert to None
                if server_info.last_updated == Some("0001-01-01T00:00:00".to_string()) {
                    server_info.last_updated = None;
                    continue;  // Skip default values too
                }
                
                // Skip user-submitted checks
                if server_info.extra.get("user_submitted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false) {
                    continue;
                }
                
                servers.push(server_info);
            },
            Err(e) => {
                error!("Failed to deserialize JSON for key {}: {}", key, e);
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

    let online_count = servers.iter().filter(|s| s.is_online()).count();
    let total_count = servers.len();

    let (num1, num2, _answer) = generate_math_problem();
    
    let template = IndexTemplate {
        servers,
        percentile_height,
        current_network: network.0,
        online_count,
        total_count,
        check_error: None,
        math_problem: (num1, num2),
    };
    
    let html = template.render().map_err(|e| {
        error!("Template rendering error: {}", e);
        actix_web::error::ErrorInternalServerError("Template rendering failed")
    })?;

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html))
}

#[get("/{network}/{host}")]
async fn server_detail(
    _redis: web::Data<redis::Client>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let (network, host) = path.into_inner();
    let safe_network = SafeNetwork::from_str(&network)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid network"))?;
    
    let key = format!("{}:{}", network, host);
    
    let mut conn = _redis.get_connection().map_err(|e| {
        error!("Redis connection error: {}", e);
        actix_web::error::ErrorInternalServerError("Redis connection failed")
    })?;
    
    let value: String = conn.get(&key).map_err(|e| {
        error!("Redis get error for key {}: {}", key, e);
        actix_web::error::ErrorInternalServerError("Failed to fetch Redis value")
    })?;
    
    let data: HashMap<String, Value> = serde_json::from_str(&value).map_err(|e| {
        error!("JSON deserialization error for key {}: {}", key, e);
        actix_web::error::ErrorInternalServerError("Failed to parse server data")
    })?;
    
    let keys: Vec<String> = conn.keys(safe_network.redis_prefix()).map_err(|e| {
        error!("Redis error: {}", e);
        actix_web::error::ErrorInternalServerError("Redis error")
    })?;
    
    let total_count = keys.len();
    let mut heights = Vec::new();

    for key in keys {
        if let Ok(Some(data)) = conn.get::<_, Option<String>>(&key) {
            if let Ok(server_data) = serde_json::from_str::<Value>(&data) {
                if let Some(height) = server_data.get("height").and_then(|h| h.as_u64()) {
                    if height > 0 {
                        heights.push(height);
                    }
                }
            }
        }
    }

    let online_count = heights.len();
    let percentile_height = calculate_percentile(&heights, 90);

    let template = ServerTemplate {
        data,
        host,
        network,
        current_network: safe_network.0,
        percentile_height,
        online_count,
        total_count,
    };
    
    let html = template.render().map_err(|e| {
        error!("Template rendering error: {}", e);
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
                "http" => (server.port.unwrap_or(80), "http"),  // Add HTTP case
                _ => unreachable!(),
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
        error!("Redis connection error: {}", e);
        actix_web::error::ErrorInternalServerError("Redis connection failed")
    })?;

    let keys: Vec<String> = conn.keys(format!("{}:*", network)).map_err(|e| {
        error!("Redis keys error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to fetch Redis keys")
    })?;

    let mut servers = Vec::new();
    for key in keys {
        let value: String = conn.get(&key).map_err(|e| {
            error!("Redis get error for key {}: {}", key, e);
            actix_web::error::ErrorInternalServerError("Failed to fetch Redis value")
        })?;

        match serde_json::from_str::<ServerInfo>(&value) {
            Ok(server_info) => servers.push(server_info),
            Err(e) => {
                error!("Failed to deserialize JSON for key {}: {}", key, e);
                println!("Retrieved JSON for key {}: {}", key, value);
            }
        }
    }

    Ok(servers)
}

fn calculate_percentile(values: &[u64], percentile: u8) -> u64 {
    if values.is_empty() {
        return 0;
    }
    
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    
    let index = (percentile as f64 / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[index]
}

#[derive(Deserialize)]
struct CheckServerForm {
    url: String,
    port: Option<u16>,
    verification: String,
    expected_answer: String,
}

#[post("/{network}/check")]
async fn check_server(
    redis: web::Data<redis::Client>,
    network: web::Path<String>,
    form: web::Form<CheckServerForm>,
) -> Result<HttpResponse> {
    let network_str = network.into_inner();
    let network = SafeNetwork::from_str(&network_str)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid network"))?;

    // Parse and verify the math answer
    let answer: u8 = form.verification.parse().unwrap_or(0);
    let (num1, num2, _expected) = generate_math_problem();  // We'll use this if verification fails
    
    if answer != form.expected_answer.parse().unwrap_or(0) {
        let servers = fetch_network_servers(&redis, network.0).await?;
        let online_count = servers.iter().filter(|s| s.is_online()).count();
        let total_count = servers.len();
        let percentile_height = calculate_percentile(
            &servers.iter()
                .filter(|s| s.height > 0)
                .map(|s| s.height)
                .collect::<Vec<_>>(), 
            90
        );

        let template = IndexTemplate {
            servers,
            percentile_height,
            current_network: network.0,
            online_count,
            total_count,
            check_error: Some("Incorrect answer, please try again"),
            math_problem: (num1, num2),
        };

        let html = template.render().map_err(|e| {
            error!("Template rendering error: {}", e);
            actix_web::error::ErrorInternalServerError("Template rendering failed")
        })?;

        return Ok(HttpResponse::BadRequest()
            .content_type("text/html; charset=utf-8")
            .body(html));
    }

    // Validate form data
    if form.url.is_empty() {
        return Ok(HttpResponse::BadRequest().body("URL is required"));
    }

    let check_id = Uuid::new_v4().to_string();
    
    // Check if this server is already in our public list
    let mut conn = redis.get_connection().map_err(|e| {
        error!("Redis connection error: {}", e);
        actix_web::error::ErrorInternalServerError("Redis connection failed")
    })?;

    let key = format!("{}:{}", network.0, &form.url);
    let existing_server: Option<ServerInfo> = conn.get(&key)
        .ok()
        .and_then(|value: String| serde_json::from_str(&value).ok());

    let is_user_submitted = existing_server
        .map(|s| s.extra.get("user_submitted")
            .and_then(|v| v.as_bool())
            .unwrap_or(true))
        .unwrap_or(true);

    let check_request = serde_json::json!({
        "host": form.url,
        "port": form.port.unwrap_or(50002),
        "user_submitted": is_user_submitted,
        "check_id": check_id
    });

    info!("📤 Submitting check request to NATS - host: {}, port: {}, check_id: {}",
        form.url, form.port.unwrap_or(50002), check_id
    );

    let nats = nats::connect("nats://nats:4222").map_err(|e| {
        error!("NATS connection error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to connect to NATS")
    })?;

    // Use different subjects for BTC vs ZEC vs HTTP
    let subject = match network.0 {
        "btc" => format!("hosh.check.btc.user"), // BTC still uses separate user queue
        "zec" => format!("hosh.check.zec"), // ZEC uses single queue for all checks
        "http" => format!("hosh.check.http"),  // HTTP case handles all explorers
        _ => unreachable!("Invalid network"),
    };
    info!("📤 Publishing to NATS subject: {}", subject);
    
    nats.publish(&subject, &serde_json::to_vec(&check_request).unwrap())
        .map_err(|e| {
            error!("NATS publish error: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to publish check request")
        })?;

    info!("✅ Successfully published check request to NATS");

    // Redirect to network-specific check result page, carrying host & port
    Ok(HttpResponse::SeeOther()
        .insert_header((
            "Location",
            format!(
                "/check/{}/{}?host={}&port={}",
                network.0,
                check_id,
                form.url,
                form.port.unwrap_or(50002)
            ),
        ))
        .finish())
}

#[get("/check/{network}/{check_id}")]
async fn check_result(
    path: web::Path<(String, String)>,
    query: web::Query<CheckQuery>,
    redis: web::Data<redis::Client>,
) -> Result<HttpResponse> {
    let (network_str, check_id) = path.into_inner();

    // Start with the query-based host/port
    let checking_url = query.host.clone();
    let checking_port = query.port;

    // We'll actually fetch from Redis to see if the checker wrote any data
    let mut conn = redis.get_connection().map_err(|e| {
        error!("Redis connection error: {}", e);
        actix_web::error::ErrorInternalServerError("Redis connection failed")
    })?;

    let prefix = format!("{}:", network_str);
    let keys: Vec<String> = conn.keys(format!("{}*", prefix)).map_err(|e| {
        error!("Redis keys error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to fetch Redis keys")
    })?;

    // We'll store the discovered server info and raw data here if we find a match
    let mut server: Option<ServerInfo> = None;
    let mut server_data: Option<HashMap<String, Value>> = None;
    let mut is_public_server = false;

    for key in keys {
        // Grab the JSON
        let value: String = conn.get(&key).map_err(|e| {
            error!("Redis get error for key {}: {}", key, e);
            actix_web::error::ErrorInternalServerError("Failed to fetch Redis value")
        })?;

        // Parse the raw JSON data
        if let Ok(data) = serde_json::from_str::<HashMap<String, Value>>(&value) {
            // Check if this key has the right check_id
            let has_check_id = data.get("check_id")
                .and_then(|v| v.as_str())
                .map(|c| c == check_id)
                .unwrap_or(false);

            if has_check_id {
                // If found a matching check_id, we use that data
                server = serde_json::from_str(&value).ok();
                server_data = Some(data);
                break;
            }

            // Otherwise, check if it's a public server
            let user_submitted = data.get("user_submitted")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            if !user_submitted && key.ends_with(&check_id) {
                is_public_server = true;
                server = serde_json::from_str(&value).ok();
                server_data = Some(data);
                break;
            }
        }
    }

    let template = CheckTemplate {
        check_id,
        server,
        network: network_str.clone(),
        is_public_server,
        checking_url,
        checking_port,
        server_data,
    };

    let html = template.render().map_err(|e| {
        error!("Template rendering error: {}", e);
        actix_web::error::ErrorInternalServerError("Template rendering failed")
    })?;

    Ok(HttpResponse::Ok().body(html))
}

// Add this function to generate a math problem
fn generate_math_problem() -> (u8, u8, u8) {
    let mut rng = rand::thread_rng();
    let a = rng.gen_range(1..10);
    let b = rng.gen_range(1..10);
    (a, b, a + b)
}

#[get("/explorers")]
async fn blockchain_heights(redis: web::Data<redis::Client>) -> Result<HttpResponse> {
    let mut con = redis.get_connection().map_err(|e| {
        eprintln!("Redis connection error: {}", e);
        actix_web::error::ErrorInternalServerError("Redis connection failed")
    })?;

    // Group heights by source
    let mut explorer_data = HashMap::new();
    let sources = ["blockchair", "blockchain", "blockstream", "zecrocks", "zcashexplorer"];
    
    for source in &sources {
        explorer_data.insert(source.to_string(), HashMap::new());
    }

    // Get all keys matching http:*
    let keys: Vec<String> = con.keys("http:*").map_err(|e| {
        eprintln!("Redis keys error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to fetch keys from Redis")
    })?;
    
    // Collect all unique chains
    let mut chains = HashSet::new();
    
    for key in keys {
        if let Ok(height) = con.get::<_, u64>(&key) {
            // Parse the key format "http:source.chain"
            let parts: Vec<&str> = key.split('.').collect();
            if parts.len() == 2 {
                let source = parts[0].replace("http:", "");
                let chain = parts[1].to_string();
                
                chains.insert(chain.clone());
                
                if let Some(heights) = explorer_data.get_mut(&source) {
                    heights.insert(chain, height);
                }
            }
        }
    }

    // Sort chains for consistent display
    let mut chains: Vec<_> = chains.into_iter().collect();
    chains.sort_unstable();

    // Build rows
    let mut rows = Vec::new();
    for chain in &chains {
        let row = ExplorerRow {
            chain: chain.clone(),
            blockchair:    explorer_data.get("blockchair").and_then(|h| h.get(chain)).copied(),
            blockchain_com: explorer_data.get("blockchain").and_then(|h| h.get(chain)).copied(),
            blockstream:   explorer_data.get("blockstream").and_then(|h| h.get(chain)).copied(),
            zecrocks:      explorer_data.get("zecrocks").and_then(|h| h.get(chain)).copied(),
            zcashexplorer: explorer_data.get("zcashexplorer").and_then(|h| h.get(chain)).copied(),
        };
        rows.push(row);
    }

    // After building rows, before creating template
    // Sort rows by number of active explorers (non-None values) in descending order
    rows.sort_by(|a, b| {
        let a_count = [
            a.blockchair.is_some(),
            a.blockchain_com.is_some(),
            a.blockstream.is_some(),
            a.zecrocks.is_some(),
            a.zcashexplorer.is_some()
        ].iter().filter(|&&x| x).count();

        let b_count = [
            b.blockchair.is_some(),
            b.blockchain_com.is_some(),
            b.blockstream.is_some(),
            b.zecrocks.is_some(),
            b.zcashexplorer.is_some()
        ].iter().filter(|&&x| x).count();

        // Sort by count descending, then by chain name ascending for ties
        b_count.cmp(&a_count).then(a.chain.cmp(&b.chain))
    });

    let template = BlockchainHeightsTemplate { rows };
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
    // Initialize tracing subscriber
    let _subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false)
        .with_ansi(true)
        .pretty()
        .init();

    let redis_host = env::var("REDIS_HOST").unwrap_or_else(|_| {
        warn!("⚠️  REDIS_HOST not set, using default 'redis'");
        "redis".to_string()
    });
    let redis_port = env::var("REDIS_PORT").unwrap_or_else(|_| {
        warn!("⚠️  REDIS_PORT not set, using default '6379'");
        "6379".to_string()
    });
    let redis_url = format!("redis://{}:{}", redis_host, redis_port);
    
    info!("🔌 Connecting to Redis at {}", redis_url);

    let redis_client = redis::Client::open(redis_url.as_str())
        .expect("Failed to create Redis client");

    info!("🚀 Starting server at http://0.0.0.0:8080");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(redis_client.clone()))
            .service(fs::Files::new("/static", "./static"))
            .service(root)
            .service(blockchain_heights)
            .service(network_status)
            .service(server_detail)
            .service(network_api)
            .service(check_server)
            .service(check_result)
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}

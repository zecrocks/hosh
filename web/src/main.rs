use std::env;
use std::collections::HashMap;
use actix_web::{get, post, web::{self, Redirect}, App, HttpResponse, HttpServer, Result};
use actix_files as fs;
use askama::Template;
use serde::{Deserialize, Serialize, de::Deserializer};
use serde::de::Error;
use serde_json::Value;
use chrono::{DateTime, Utc, FixedOffset};
use uuid::Uuid;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn, error, Level, debug};
use tracing_subscriber::FmtSubscriber;
use reqwest;
use async_nats;

mod filters {
    use askama::Result;
    use serde_json::Value;

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

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    servers: Vec<ServerInfo>,
    percentile_height: u64,
    current_network: &'static str,
    online_count: usize,
    total_count: usize,
    check_error: Option<&'a str>,
    math_problem: (u8, u8, u8),
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
    status: String,

    #[serde(default, deserialize_with = "deserialize_error_field")]
    error: Option<String>,

    #[serde(default)]
    error_type: Option<String>,

    #[serde(default)]
    error_message: Option<String>,

    #[serde(default)]
    last_updated: Option<String>,

    #[serde(default)]
    ping: Option<f64>,

    #[serde(default)]
    server_version: Option<String>,

    #[serde(default)]
    user_submitted: bool,

    #[serde(default)]
    check_id: Option<String>,

    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

fn deserialize_port<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        Value::Number(n) => n.as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .map(Some)
            .or(Some(None))
            .ok_or_else(|| D::Error::custom("Invalid port number")),
        Value::String(s) => {
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<u16>()
                    .map(Some)
                    .or(Ok(None))
            }
        },
        Value::Null => Ok(None),
        _ => {
            warn!("Unexpected port value format: {:?}", value);
            Ok(None)
        }
    }
}

fn deserialize_error_field<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    
    match value {
        // Handle null as None
        serde_json::Value::Null => Ok(None),
        
        // Handle direct strings
        serde_json::Value::String(s) => {
            if s.is_empty() {
                return Ok(None);
            }
            
            // Clean up common error messages
            let cleaned = s
                .replace("\\n", " ")
                .replace("\\r", " ")
                .replace("\\t", " ")
                .replace("\\\"", "\"")
                .replace("\\\\", "\\")
                .trim()
                .to_string();
                
            // Extract error message from Status structure if present
            let error_msg = if cleaned.contains("Status {") {
                if let Some(start) = cleaned.find("message: \"") {
                    let start = start + 10; // Skip "message: \""
                    if let Some(end) = cleaned[start..].find("\", source:") {
                        cleaned[start..start + end].to_string()
                    } else if let Some(end) = cleaned[start..].find("\"") {
                        cleaned[start..start + end].to_string()
                    } else {
                        cleaned
                    }
                } else {
                    cleaned
                }
            } else {
                cleaned
            };
                
            // Map common error messages to more user-friendly versions
            let mapped = if error_msg.contains("tls handshake eof") {
                "TLS handshake failed - server may be offline or not accepting connections".to_string()
            } else if error_msg.contains("connection refused") {
                "Connection refused - server may be offline or not accepting connections".to_string()
            } else if error_msg.contains("InvalidContentType") {
                "Invalid content type - server may not be a valid Zcash node".to_string()
            } else {
                error_msg
            };
            
            Ok(Some(mapped))
        },
        
        // Handle objects that might contain error messages
        serde_json::Value::Object(obj) => {
            // Try to extract error message from common fields
            let error_msg = obj.get("error")
                .or_else(|| obj.get("message"))
                .or_else(|| obj.get("detail"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
                
            if let Some(msg) = error_msg {
                Ok(Some(msg))
            } else {
                // If no error message found, return None
                Ok(None)
            }
        },
        
        // Everything else is treated as None
        _ => Ok(None),
    }
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

    fn is_online(&self) -> bool {
        self.status == "success" && self.height > 0
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

    fn formatted_version(&self) -> String {
        let lwd_version = self.server_version
            .as_ref()
            .map(String::as_str)
            .unwrap_or("-")
            .to_string();
        
        // Hacky check to see if the server is running Zaino (doesn't start with "v")
        let lwd_display = if !lwd_version.is_empty() && lwd_version != "-" && !lwd_version.starts_with('v') {
            // Only show Zaino indicator for ZEC currency
            if self.extra.get("zcashd_subversion").is_some() {
                format!("{} (Zaino 🚀)", lwd_version)
            } else {
                lwd_version
            }
        } else {
            lwd_version
        };

        // Display both LWD and Zebra versions for ZEC if available
        if let Some(subversion) = self.extra.get("zcashd_subversion") {
            if let Some(subversion_str) = subversion.as_str() {
                // Remove slashes from subversion string
                let cleaned_subversion = subversion_str.replace('/', "");
                return format!("{}\nLWD: {}", cleaned_subversion, lwd_display);
            }
        }
        
        lwd_display
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
#[template(path = "check.html")]
struct CheckTemplate {
    check_id: String,
    server: Option<ServerInfo>,
    network: String,
    checking_url: Option<String>,
    checking_port: Option<u16>,
    server_data: Option<HashMap<String, Value>>,
}

impl CheckTemplate {
    fn network_upper(&self) -> String {
        self.network.to_uppercase()
    }

    fn is_checking(&self) -> bool {
        self.server.is_none()
    }

    fn has_error(&self) -> bool {
        self.server.as_ref()
            .map(|s| s.error.is_some())
            .unwrap_or(false)
    }

    fn error_message(&self) -> Option<&str> {
        self.server.as_ref()
            .and_then(|s| s.error.as_deref())
    }
}

#[derive(Deserialize)]
struct CheckQuery {
    host: Option<String>,
    port: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct ExplorerRow {
    chain: String,
    explorer: String,
    #[serde(deserialize_with = "deserialize_string_or_number")]
    block_height: Option<u64>,
}

#[derive(Template)]
#[template(path = "blockchain_heights.html")]
struct BlockchainHeightsTemplate {
    rows: Vec<ExplorerRow>,
}

impl BlockchainHeightsTemplate {
    fn format_chain_name(&self, chain: &str) -> String {
        chain.replace("-", " ")
    }

    fn get_unique_chains(&self) -> Vec<&str> {
        // First collect chains and their active explorer counts
        let mut chains_with_counts: Vec<(&str, usize)> = self.rows.iter()
            .map(|row| row.chain.as_str())
            .collect::<std::collections::HashSet<_>>()  // Get unique chains
            .into_iter()
            .map(|chain| {
                // Count non-empty heights for this chain
                let active_count = self.rows.iter()
                    .filter(|row| row.chain == chain && row.block_height.is_some())
                    .count();
                (chain, active_count)
            })
            .collect();

        // Sort by number of active explorers (descending), then alphabetically by chain name
        chains_with_counts.sort_by(|a, b| {
            b.1.cmp(&a.1)  // Sort by count descending
                .then(a.0.cmp(&b.0))  // Then alphabetically by chain name
        });

        // Return just the chain names in sorted order
        chains_with_counts.into_iter()
            .map(|(chain, _)| chain)
            .collect()
    }

    fn get_unique_explorers(&self) -> Vec<&str> {
        // First collect explorers and their chain counts
        let mut explorers_with_counts: Vec<(&str, usize)> = self.rows.iter()
            .map(|row| row.explorer.as_str())
            .collect::<std::collections::HashSet<_>>()  // Get unique explorers
            .into_iter()
            .map(|explorer| {
                // Count how many chains this explorer tracks
                let chain_count = self.rows.iter()
                    .filter(|row| row.explorer == explorer && row.block_height.is_some())
                    .map(|row| &row.chain)
                    .collect::<std::collections::HashSet<_>>()
                    .len();
                (explorer, chain_count)
            })
            .collect();

        // Sort by number of chains tracked (descending), then alphabetically by explorer name
        explorers_with_counts.sort_by(|a, b| {
            b.1.cmp(&a.1)  // Sort by count descending
                .then(a.0.cmp(&b.0))  // Then alphabetically by explorer name
        });

        // Return just the explorer names in sorted order
        explorers_with_counts.into_iter()
            .map(|(explorer, _)| explorer)
            .collect()
    }

    fn get_chain_logo(&self, chain: &str) -> String {
        // Use the old Blockchair URL format for chain logos, with ⛓ as fallback
        format!("https://loutre.blockchair.io/w4/assets/images/blockchains/{}/logo_light_48.webp", chain)
    }

    fn get_explorer_logo(&self, explorer: &str) -> String {
        match explorer {
            "blockchair" => "https://blockchair.com/favicon.ico",
            "blockchain" => "https://www.blockchain.com/favicon.ico", 
            "blockstream" => "https://blockstream.info/favicon.ico",
            "zecrocks" => "https://explorer.zec.rocks/favicon.ico",
            "zcashexplorer" => "https://mainnet.zcashexplorer.app/favicon.ico",
            _ => "⛓" // Use chain symbol instead of default favicon
        }.to_string()
    }

    fn get_explorer_url(&self, explorer: &str) -> String {
        match explorer {
            "blockchair" => "https://blockchair.com",
            "blockchain" => "https://www.blockchain.com/explorer",
            "blockstream" => "https://blockstream.info",
            "zecrocks" => "https://explorer.zec.rocks",
            "zcashexplorer" => "https://mainnet.zcashexplorer.app",
            _ => "#" // Default fallback
        }.to_string()
    }

    fn get_chain_height(&self, chain: &str, explorer: &str) -> Option<(u64, Option<String>)> {
        let row = self.rows.iter()
            .find(|r| r.chain == chain && r.explorer == explorer)?;
        
        let height = row.block_height?;
        
        // Calculate difference if there are multiple heights for this chain
        let diff = self.get_height_difference(height, chain);
        
        Some((height, diff))
    }

    fn get_height_difference(&self, height: u64, chain: &str) -> Option<String> {
        let chain_heights: Vec<u64> = self.rows.iter()
            .filter(|row| row.chain == chain)
            .filter_map(|row| row.block_height)
            .collect();
            
        if chain_heights.len() >= 2 {
            let min_height = chain_heights.iter().min()?;
            let diff = height.saturating_sub(*min_height);
            if diff > 0 {
                return Some(format!(" (+{})", diff));
            }
        }
        None
    }
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
struct Config {
    results_window_days: u64,
}

impl Config {
    fn from_env() -> Result<Self, actix_web::Error> {
        let results_window_days = env::var("RESULTS_WINDOW_DAYS")
            .unwrap_or_else(|_| "1".to_string())
            .parse()
            .map_err(|e| {
                warn!("Failed to parse RESULTS_WINDOW_DAYS: {}", e);
                actix_web::error::ErrorBadRequest(format!("Invalid RESULTS_WINDOW_DAYS value: {}", e))
            })?;

        Ok(Self {
            results_window_days,
        })
    }
}

#[derive(Clone)]
struct Worker {
    #[allow(dead_code)]
    nats: async_nats::Client,
    clickhouse: ClickhouseConfig,
    http_client: reqwest::Client,
    config: Config,
}

#[get("/")]
async fn root() -> Result<Redirect> {
    Ok(Redirect::to("/zec"))
}

#[get("/{network}")]
async fn network_status(
    worker: web::Data<Worker>,
    network: web::Path<String>,
) -> Result<HttpResponse> {
    let network = SafeNetwork::from_str(&network)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid network"))?;

    // Update query to handle empty results and use FORMAT JSONEachRow
    let query = format!(
        r#"
        WITH latest_results AS (
            SELECT 
                r.*,
                ROW_NUMBER() OVER (PARTITION BY r.hostname ORDER BY r.checked_at DESC) as rn
            FROM {}.results r
            WHERE r.checker_module = '{}'
            AND r.checked_at >= now() - INTERVAL {} DAY
        )
        SELECT 
            hostname,
            checked_at,
            status,
            ping_ms as ping,
            response_data
        FROM latest_results
        WHERE rn = 1
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database,
        network.0,
        worker.config.results_window_days
    );

    info!(
        "Executing ClickHouse query for network {} with window of {} days", 
        network.0,
        worker.config.results_window_days
    );

    let response = worker.http_client.post(&worker.clickhouse.url)
        .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
        .header("Content-Type", "text/plain")
        .body(query.clone())
        .send()
        .await
        .map_err(|e| {
            error!("ClickHouse query error: {}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    let status = response.status();
    let body = response.text().await.map_err(|e| {
        error!("Failed to read response body: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to read database response")
    })?;

    if !status.is_success() {
        error!("ClickHouse query failed with status {}: {}", status, body);
        return Err(actix_web::error::ErrorInternalServerError("Database query failed"));
    }

    // Handle empty response case
    if body.trim().is_empty() {
        info!("No results found for network {}", network.0);
        let template = IndexTemplate {
            servers: Vec::new(),
            percentile_height: 0,
            current_network: network.0,
            online_count: 0,
            total_count: 0,
            check_error: None,
            math_problem: generate_math_problem(),
        };

        let html = template.render().map_err(|e| {
            error!("Template rendering error: {}", e);
            actix_web::error::ErrorInternalServerError("Template rendering failed")
        })?;

        return Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(html));
    }

    // Parse results line by line (JSONEachRow format)
    let mut servers = Vec::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Log the raw response for debugging
        debug!("Raw server response: {}", line);
        
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(result) => {
                // Get response_data, with better error handling
                let response_data = match result.get("response_data") {
                    Some(val) => match val {
                        Value::String(s) => s,
                        _ => {
                            error!("response_data is not a string: {:?}", val);
                            "{}"
                        }
                    },
                    None => {
                        error!("No response_data field in response: {:?}", result);
                        "{}"
                    }
                };
                
                // Now parse that string into ServerInfo with better error reporting
                match serde_json::from_str::<ServerInfo>(response_data) {
                    Ok(server_info) => {
                        servers.push(server_info);
                    }
                    Err(e) => {
                        error!(
                            "Failed to parse server info for host {}: {} (raw data: {})", 
                            result["hostname"].as_str().unwrap_or("unknown"),
                            e,
                            response_data
                        );
                    }
                }
            }
            Err(e) => {
                error!("Failed to parse JSON line: {} (raw line: {})", e, line);
            }
        }
    }

    // Calculate percentile height
    let heights: Vec<u64> = servers.iter()
        .filter(|s| s.height > 0)
        .map(|s| s.height)
        .collect();
    let percentile_height = calculate_percentile(&heights, 90);

    let online_count = servers.iter().filter(|s| s.is_online()).count();
    let total_count = servers.len();

    let template = IndexTemplate {
        servers,
        percentile_height,
        current_network: network.0,
        online_count,
        total_count,
        check_error: None,
        math_problem: generate_math_problem(),
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
    worker: web::Data<Worker>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let (network, host) = path.into_inner();
    let safe_network = SafeNetwork::from_str(&network)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid network"))?;
    
    // Query the targets table to get server information
    let query = format!(
        r#"
        WITH latest_results AS (
            SELECT 
                r.*,
                ROW_NUMBER() OVER (PARTITION BY r.hostname ORDER BY r.checked_at DESC) as rn
            FROM {}.results r
            WHERE r.checker_module = '{}'
            AND r.hostname = '{}'
            AND r.checked_at >= now() - INTERVAL {} DAY
        )
        SELECT 
            hostname,
            checked_at,
            status,
            ping_ms as ping,
            response_data
        FROM latest_results
        WHERE rn = 1
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database,
        safe_network.0,
        host,
        worker.config.results_window_days
    );

    let response = worker.http_client.post(&worker.clickhouse.url)
        .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
        .header("Content-Type", "text/plain")
        .body(query.clone())
        .send()
        .await
        .map_err(|e| {
            error!("ClickHouse query error: {}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    let status = response.status();
    let body = response.text().await.map_err(|e| {
        error!("Failed to read response body: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to read database response")
    })?;

    if !status.is_success() {
        error!("ClickHouse query failed with status {}: {}", status, body);
        return Err(actix_web::error::ErrorInternalServerError("Database query failed"));
    }

    // Parse the response data
    let mut data: HashMap<String, Value> = HashMap::new();
    if !body.trim().is_empty() {
        if let Ok(result) = serde_json::from_str::<serde_json::Value>(body.lines().next().unwrap()) {
            if let Some(response_data) = result["response_data"].as_str() {
                if let Ok(parsed_data) = serde_json::from_str::<HashMap<String, Value>>(response_data) {
                    data = parsed_data;
                }
            }
        }
    }

    // Get total count and heights for percentile calculation
    let count_query = format!(
        r#"
        WITH latest_results AS (
            SELECT 
                r.*,
                ROW_NUMBER() OVER (PARTITION BY r.hostname ORDER BY r.checked_at DESC) as rn
            FROM {}.results r
            WHERE r.checker_module = '{}'
            AND r.checked_at >= now() - INTERVAL 1 DAY
        )
        SELECT 
            hostname,
            response_data
        FROM latest_results
        WHERE rn = 1
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database,
        safe_network.0
    );

    let count_response = worker.http_client.post(&worker.clickhouse.url)
        .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
        .header("Content-Type", "text/plain")
        .body(count_query)
        .send()
        .await
        .map_err(|e| {
            error!("ClickHouse query error: {}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    let count_body = count_response.text().await.map_err(|e| {
        error!("Failed to read response body: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to read database response")
    })?;

    let mut heights = Vec::new();
    let mut total_count = 0;

    for line in count_body.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(result) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(response_data) = result["response_data"].as_str() {
                if let Ok(server_data) = serde_json::from_str::<Value>(response_data) {
                    if let Some(height) = server_data.get("height").and_then(|h| h.as_u64()) {
                        if height > 0 {
                            heights.push(height);
                        }
                    }
                }
            }
        }
        total_count += 1;
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
    worker: web::Data<Worker>,
    network: web::Path<String>,
) -> Result<HttpResponse> {
    let network = SafeNetwork::from_str(&network)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid network"))?;
    
    // Query the results table to get all servers for the network
    let query = format!(
        r#"
        WITH latest_results AS (
            SELECT 
                r.*,
                ROW_NUMBER() OVER (PARTITION BY r.hostname ORDER BY r.checked_at DESC) as rn
            FROM {}.results r
            WHERE r.checker_module = '{}'
            AND r.checked_at >= now() - INTERVAL {} DAY
        )
        SELECT 
            hostname,
            checked_at,
            status,
            ping_ms as ping,
            response_data
        FROM latest_results
        WHERE rn = 1
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database,
        network.0,
        worker.config.results_window_days
    );

    let response = worker.http_client.post(&worker.clickhouse.url)
        .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
        .header("Content-Type", "text/plain")
        .body(query.clone())
        .send()
        .await
        .map_err(|e| {
            error!("ClickHouse query error: {}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    let status = response.status();
    let body = response.text().await.map_err(|e| {
        error!("Failed to read response body: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to read database response")
    })?;

    if !status.is_success() {
        error!("ClickHouse query failed with status {}: {}", status, body);
        return Err(actix_web::error::ErrorInternalServerError("Database query failed"));
    }

    let mut servers = Vec::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(result) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(response_data) = result["response_data"].as_str() {
                if let Ok(server_info) = serde_json::from_str::<ServerInfo>(response_data) {
                    servers.push(server_info);
                }
            }
        }
    }
    
    let api_servers: Vec<ApiServerInfo> = servers.into_iter()
        .map(|server| {
            let (port, protocol) = match network.0 {
                "btc" => (server.port.unwrap_or(50002), "ssl"),
                "zec" => (server.port.unwrap_or(443), "grpc"),
                "http" => (server.port.unwrap_or(80), "http"),
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
    worker: web::Data<Worker>,
    network: web::Path<String>,
    form: web::Form<CheckServerForm>,
) -> Result<HttpResponse> {
    let network_str = network.into_inner();
    let network = SafeNetwork::from_str(&network_str)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid network"))?;

    // Parse and verify the math answer
    let answer: u8 = form.verification.parse().unwrap_or(0);
    let (num1, num2, _expected) = generate_math_problem();
    
    if answer != form.expected_answer.parse().unwrap_or(0) {
        // Query ClickHouse for server list
        let query = format!(
            r#"
            WITH latest_results AS (
                SELECT 
                    r.*,
                    ROW_NUMBER() OVER (PARTITION BY r.hostname ORDER BY r.checked_at DESC) as rn
                FROM {}.results r
                WHERE r.checker_module = '{}'
                AND r.checked_at >= now() - INTERVAL 1 DAY
            )
            SELECT 
                hostname,
                checked_at,
                status,
                ping_ms as ping,
                response_data
            FROM latest_results
            WHERE rn = 1
            FORMAT JSONEachRow
            "#,
            worker.clickhouse.database,
            network.0
        );

        let response = worker.http_client.post(&worker.clickhouse.url)
            .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
            .header("Content-Type", "text/plain")
            .body(query)
            .send()
            .await
            .map_err(|e| {
                error!("ClickHouse query error: {}", e);
                actix_web::error::ErrorInternalServerError("Database query failed")
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|e| {
            error!("Failed to read response body: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to read database response")
        })?;

        if !status.is_success() {
            error!("ClickHouse query failed with status {}: {}", status, body);
            return Err(actix_web::error::ErrorInternalServerError("Database query failed"));
        }

        let mut servers = Vec::new();
        for line in body.lines() {
            if line.trim().is_empty() {
                continue;
            }

            if let Ok(result) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(response_data) = result["response_data"].as_str() {
                    if let Ok(server_info) = serde_json::from_str::<ServerInfo>(response_data) {
                        servers.push(server_info);
                    }
                }
            }
        }

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
            math_problem: (num1, num2, 0),
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
    let query = format!(
        r#"
        WITH latest_results AS (
            SELECT 
                r.*,
                ROW_NUMBER() OVER (PARTITION BY r.hostname ORDER BY r.checked_at DESC) as rn
            FROM {}.results r
            WHERE r.checker_module = '{}'
            AND r.hostname = '{}'
            AND r.checked_at >= now() - INTERVAL 1 DAY
        )
        SELECT 
            response_data
        FROM latest_results
        WHERE rn = 1
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database,
        network.0,
        form.url
    );

    let response = worker.http_client.post(&worker.clickhouse.url)
        .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
        .header("Content-Type", "text/plain")
        .body(query)
        .send()
        .await
        .map_err(|e| {
            error!("ClickHouse query error: {}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    let status = response.status();
    let body = response.text().await.map_err(|e| {
        error!("Failed to read response body: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to read database response")
    })?;

    let is_user_submitted = if status.is_success() && !body.trim().is_empty() {
        if let Ok(result) = serde_json::from_str::<serde_json::Value>(body.lines().next().unwrap()) {
            if let Some(response_data) = result["response_data"].as_str() {
                if let Ok(server_data) = serde_json::from_str::<Value>(response_data) {
                    server_data.get("user_submitted")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true)
                } else {
                    true
                }
            } else {
                true
            }
        } else {
            true
        }
    } else {
        true
    };

    let check_request = serde_json::json!({
        "host": form.url,
        "port": form.port.unwrap_or(50002),
        "user_submitted": is_user_submitted,
        "check_id": check_id
    });

    info!("📤 Submitting check request to NATS - host: {}, port: {}, check_id: {}, user_submitted: {}",
        form.url, form.port.unwrap_or(50002), check_id, is_user_submitted
    );

    // Log the full JSON payload for debugging
    info!("📦 Check request payload: {}", serde_json::to_string(&check_request).unwrap_or_default());

    let nats_url = format!("nats://{}:{}",
        env::var("NATS_HOST").unwrap_or_else(|_| "nats".to_string()),
        env::var("NATS_PORT").unwrap_or_else(|_| "4222".to_string())
    );

    let nats = async_nats::connect(&nats_url).await.map_err(|e| {
        error!("NATS connection error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to connect to NATS")
    })?;

    // Use different subjects for BTC vs ZEC vs HTTP
    let subject = match network.0 {
        "btc" => format!("hosh.check.btc.user"), // BTC uses separate user queue
        "zec" => format!("hosh.check.zec"),
        "http" => format!("hosh.check.http"),
        _ => unreachable!("Invalid network"),
    };
    info!("📤 Publishing to NATS subject: {}", subject);
    
    nats.publish(subject, serde_json::to_vec(&check_request).unwrap().into()).await
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
    worker: web::Data<Worker>,
) -> Result<HttpResponse> {
    let (network_str, check_id) = path.into_inner();

    // Start with the query-based host/port
    let checking_url = query.host.clone();
    let checking_port = query.port;

    info!("🔍 Looking up check result - network: {}, check_id: {}, host: {:?}, port: {:?}",
        network_str, check_id, checking_url, checking_port);

    // Query ClickHouse for the check result
    let query = format!(
        r#"
        WITH latest_results AS (
            SELECT 
                r.*,
                ROW_NUMBER() OVER (PARTITION BY r.hostname ORDER BY r.checked_at DESC) as rn
            FROM {}.results r
            WHERE r.checker_module = '{}'
            AND r.checked_at >= now() - INTERVAL 1 DAY
        )
        SELECT 
            hostname,
            checked_at,
            status,
            ping_ms as ping,
            response_data
        FROM latest_results
        WHERE rn = 1
        AND response_data LIKE '%"check_id":"{}"%'
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database,
        network_str,
        check_id
    );

    info!("🔎 ClickHouse query for result: {}", query.replace("\n", " "));

    let response = worker.http_client.post(&worker.clickhouse.url)
        .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
        .header("Content-Type", "text/plain")
        .body(query)
        .send()
        .await
        .map_err(|e| {
            error!("ClickHouse query error: {}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    let status = response.status();
    let body = response.text().await.map_err(|e| {
        error!("Failed to read response body: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to read database response")
    })?;

    info!("📊 ClickHouse response status: {}, body length: {}, empty?: {}", 
        status, body.len(), body.trim().is_empty());
    
    if !body.trim().is_empty() {
        info!("📄 First line of response: {}", body.lines().next().unwrap_or(""));
    }

    let mut server: Option<ServerInfo> = None;
    let mut server_data: Option<HashMap<String, Value>> = None;

    if status.is_success() && !body.trim().is_empty() {
        if let Ok(result) = serde_json::from_str::<serde_json::Value>(body.lines().next().unwrap()) {
            if let Some(response_data) = result["response_data"].as_str() {
                server = serde_json::from_str(response_data).ok();
                server_data = serde_json::from_str(response_data).ok();
                info!("✅ Found check result for check_id: {}", check_id);
            }
        }
    } else {
        // If no results found with LIKE pattern, try a broader search to debug
        info!("❌ No check results found with check_id LIKE pattern, trying hostname-based lookup");
        
        if let Some(host) = &checking_url {
            let backup_query = format!(
                r#"
                WITH latest_results AS (
                    SELECT 
                        r.*,
                        ROW_NUMBER() OVER (PARTITION BY r.hostname ORDER BY r.checked_at DESC) as rn
                    FROM {}.results r
                    WHERE r.checker_module = '{}'
                    AND r.hostname = '{}'
                    AND r.checked_at >= now() - INTERVAL 5 DAY
                )
                SELECT 
                    hostname,
                    checked_at,
                    status,
                    ping_ms as ping,
                    response_data
                FROM latest_results
                WHERE rn = 1
                FORMAT JSONEachRow
                "#,
                worker.clickhouse.database,
                network_str,
                host
            );
            
            info!("🔎 Backup ClickHouse query: {}", backup_query.replace("\n", " "));
            
            match worker.http_client.post(&worker.clickhouse.url)
                .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
                .header("Content-Type", "text/plain")
                .body(backup_query)
                .send()
                .await {
                    Ok(backup_response) => {
                        if let Ok(backup_body) = backup_response.text().await {
                            info!("📊 Backup query response length: {}, empty?: {}", 
                                backup_body.len(), backup_body.trim().is_empty());
                            
                            if !backup_body.trim().is_empty() {
                                if let Ok(result) = serde_json::from_str::<serde_json::Value>(backup_body.lines().next().unwrap()) {
                                    if let Some(response_data) = result["response_data"].as_str() {
                                        info!("🔍 Debug: Found response data in backup query: {}", 
                                            if response_data.len() > 100 { &response_data[..100] } else { response_data });
                                        
                                        // Check if the response contains our check_id
                                        if response_data.contains(&check_id) {
                                            info!("✅ Backup query found our check_id! This indicates a LIKE pattern issue.");
                                            server = serde_json::from_str(response_data).ok();
                                            server_data = serde_json::from_str(response_data).ok();
                                        } else {
                                            info!("❌ Response contains data but not our check_id");
                                        }
                                    }
                                }
                            }
                        }
                    },
                    Err(e) => {
                        info!("❌ Backup query failed: {}", e);
                    }
            }
        }
    }

    let template = CheckTemplate {
        check_id,
        server,
        network: network_str.clone(),
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

// Replace the generate_math_problem function with this simpler version
fn generate_math_problem() -> (u8, u8, u8) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    
    // Use the last few digits of the timestamp to generate numbers
    let a = ((timestamp % 9) + 1) as u8;
    let b = (((timestamp / 10) % 9) + 1) as u8;
    (a, b, a + b)
}

#[get("/explorers")]
async fn blockchain_heights(worker: web::Data<Worker>) -> Result<HttpResponse> {
    let query = format!(
        r#"
        WITH latest_heights AS (
            SELECT 
                explorer,
                chain,
                block_height,
                response_time_ms,
                error,
                ROW_NUMBER() OVER (PARTITION BY explorer, chain ORDER BY checked_at DESC) as rn
            FROM {}.block_explorer_heights
            WHERE checked_at >= now() - INTERVAL 1 DAY
        ),
        chain_stats AS (
            SELECT 
                chain,
                countIf(block_height IS NOT NULL AND block_height != 0) as active_explorers,
                count(*) as total_explorers
            FROM latest_heights
            WHERE rn = 1
            GROUP BY chain
        )
        SELECT 
            h.explorer,
            h.chain,
            h.block_height,
            h.response_time_ms,
            h.error
        FROM latest_heights h
        JOIN chain_stats s ON h.chain = s.chain
        WHERE h.rn = 1
        ORDER BY 
            s.active_explorers DESC,           -- Sort by number of active explorers first
            s.total_explorers DESC,            -- Then by total explorers
            h.chain ASC,                       -- Then alphabetically by chain
            h.explorer ASC                     -- Finally by explorer name
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database
    );

    let response = worker.http_client.post(&worker.clickhouse.url)
        .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
        .header("Content-Type", "text/plain")
        .body(query.clone())
        .send()
        .await
        .map_err(|e| {
            error!("ClickHouse query error: {}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    let status = response.status();
    let body = response.text().await.map_err(|e| {
        error!("Failed to read response body: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to read database response")
    })?;

    if !status.is_success() {
        error!("ClickHouse query failed with status {}: {}", status, body);
        return Err(actix_web::error::ErrorInternalServerError("Database query failed"));
    }

    if body.trim().is_empty() {
        info!("No block explorer heights found");
        let template = BlockchainHeightsTemplate { rows: Vec::new() };
        let html = template.render().map_err(|e| {
            error!("Template rendering error: {}", e);
            actix_web::error::ErrorInternalServerError("Template rendering failed")
        })?;

        return Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(html));
    }

    // Parse results line by line
    let mut rows = Vec::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<ExplorerRow>(line) {
            Ok(row) => rows.push(row),
            Err(e) => {
                error!("Failed to parse explorer row: {}", e);
            }
        }
    }

    let template = BlockchainHeightsTemplate { rows };
    let html = template.render().map_err(|e| {
        error!("Template rendering error: {}", e);
        actix_web::error::ErrorInternalServerError("Template rendering failed")
    })?;

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html))
}

// Add this deserialization function near the other deserialize functions
fn deserialize_string_or_number<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    
    match value {
        // Handle direct numbers
        serde_json::Value::Number(n) => n.as_u64().map(Some).ok_or_else(|| {
            serde::de::Error::custom("Invalid number format")
        }),
        
        // Handle strings that contain numbers
        serde_json::Value::String(s) => {
            if s.is_empty() {
                return Ok(None);
            }
            s.parse().map(Some).map_err(|_| {
                serde::de::Error::custom("Failed to parse string as number")
            })
        },
        
        // Handle null as None
        serde_json::Value::Null => Ok(None),
        
        // Everything else is an error
        _ => Err(serde::de::Error::custom("Expected number or string")),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize tracing subscriber
    let _subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false)
        .with_ansi(true)
        .pretty()
        .init();
    
    let nats_url = format!("nats://{}:{}",
        env::var("NATS_HOST").unwrap_or_else(|_| "nats".to_string()),
        env::var("NATS_PORT").unwrap_or_else(|_| "4222".to_string())
    );

    let http_client = reqwest::Client::builder()
        .pool_idle_timeout(std::time::Duration::from_secs(300))
        .pool_max_idle_per_host(32)
        .tcp_keepalive(std::time::Duration::from_secs(60))
        .build()
        .expect("Failed to create HTTP client");

    let config = Config::from_env().expect("Failed to load config from environment");

    let worker = Worker {
        nats: async_nats::connect(&nats_url).await.expect("Failed to connect to NATS"),
        clickhouse: ClickhouseConfig::from_env(),
        http_client,
        config,
    };

    info!("🚀 Starting server at http://0.0.0.0:8080");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(worker.clone()))
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

use std::env;
use std::collections::HashMap;
use actix_web::{get, post, web::{self, Redirect}, App, HttpResponse, HttpServer, Result};
use actix_files as fs;
use askama::Template;
use serde::{Deserialize, Serialize};
use serde::de::Error;
use serde_json::Value;
use chrono::{DateTime, Utc, FixedOffset};
use uuid::Uuid;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn, error};
use tracing_subscriber;
use reqwest;
use async_nats;
use regex;
use qrcode::{QrCode, render::svg};

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
    #[serde(default, deserialize_with = "deserialize_host")]
    host: String,

    #[serde(default, deserialize_with = "deserialize_port")]
    port: Option<u16>,

    #[serde(default, deserialize_with = "deserialize_height")]
    height: u64,

    #[serde(default)]
    status: String,

    #[serde(default, deserialize_with = "deserialize_error_field")]
    error: Option<String>,

    #[serde(default)]
    error_type: Option<String>,

    #[serde(default, deserialize_with = "deserialize_error_message")]
    error_message: Option<String>,

    #[serde(default, deserialize_with = "deserialize_ping")]
    ping: Option<f64>,

    #[serde(default, deserialize_with = "deserialize_server_version")]
    server_version: Option<String>,

    #[serde(default, deserialize_with = "deserialize_user_submitted")]
    user_submitted: bool,

    #[serde(default)]
    check_id: Option<String>,

    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,

    #[serde(default)]
    last_updated: Option<String>,

    #[serde(default)]
    uptime_30_day: Option<f64>,
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

fn deserialize_host<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        Value::String(s) => {
            // Remove surrounding quotes if present
            let clean_host = s.trim_matches('\'');
            Ok(clean_host.to_string())
        },
        Value::Null => Ok(String::new()),
        _ => {
            warn!("Unexpected host value format: {:?}", value);
            Ok(String::new())
        }
    }
}

fn deserialize_height<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        Value::Number(n) => n.as_u64()
            .ok_or_else(|| D::Error::custom("Invalid height number")),
        Value::String(s) => {
            if s.is_empty() {
                Ok(0)
            } else {
                s.parse::<u64>()
                    .map_err(|_| D::Error::custom("Failed to parse height string as number"))
            }
        },
        Value::Null => Ok(0),
        _ => {
            warn!("Unexpected height value format: {:?}", value);
            Ok(0)
        }
    }
}

fn deserialize_ping<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        Value::Number(n) => n.as_f64()
            .map(Some)
            .ok_or_else(|| D::Error::custom("Invalid ping number")),
        Value::String(s) => {
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<f64>()
                    .map(Some)
                    .map_err(|_| D::Error::custom("Failed to parse ping string as number"))
            }
        },
        Value::Null => Ok(None),
        _ => {
            warn!("Unexpected ping value format: {:?}", value);
            Ok(None)
        }
    }
}

fn deserialize_user_submitted<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        Value::Bool(b) => Ok(b),
        Value::String(s) => {
            match s.to_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => Ok(true),
                "false" | "0" | "no" | "off" => Ok(false),
                _ => {
                    warn!("Unexpected user_submitted string value: {:?}", s);
                    Ok(false) // Default to false for unknown values
                }
            }
        },
        Value::Number(n) => {
            if let Some(num) = n.as_u64() {
                Ok(num != 0)
            } else {
                warn!("Unexpected user_submitted number value: {:?}", n);
                Ok(false)
            }
        },
        Value::Null => Ok(false),
        _ => {
            warn!("Unexpected user_submitted value format: {:?}", value);
            Ok(false)
        }
    }
}

fn deserialize_server_version<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        Value::String(s) => {
            // Remove surrounding quotes if present
            let clean_version = s.trim_matches('\'');
            if clean_version.is_empty() {
                Ok(None)
            } else {
                Ok(Some(clean_version.to_string()))
            }
        },
        Value::Null => Ok(None),
        _ => {
            warn!("Unexpected server_version value format: {:?}", value);
            Ok(None)
        }
    }
}

/// Clean and escape error messages to prevent JSON parsing issues
fn clean_error_message(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    
    let mut cleaned = input.to_string();
    
    // First, handle common escape sequences
    cleaned = cleaned
        .replace("\\n", " ")
        .replace("\\r", " ")
        .replace("\\t", " ")
        .replace("\\\"", "\"")
        .replace("\\\\", "\\");
    
    // Remove or replace problematic characters that break JSON
    cleaned = cleaned
        .replace("\"", "'")  // Replace unescaped quotes with single quotes
        .replace("{", "(")   // Replace unescaped braces with parentheses
        .replace("}", ")")
        .replace("[", "(")   // Replace unescaped brackets with parentheses
        .replace("]", ")");
    
    // Clean up multiple spaces
    while cleaned.contains("  ") {
        cleaned = cleaned.replace("  ", " ");
    }
    
    // Trim whitespace
    cleaned = cleaned.trim().to_string();
    
    // If the message is too long, truncate it
    if cleaned.len() > 200 {
        cleaned = cleaned.chars().take(197).collect::<String>() + "...";
    }
    
    cleaned
}

/// Validate and attempt to fix malformed JSON strings
fn validate_and_fix_json(input: &str) -> Option<String> {
    if input.trim().is_empty() {
        return None;
    }
    
    // First, try to parse as-is
    if serde_json::from_str::<serde_json::Value>(input).is_ok() {
        return Some(input.to_string());
    }
    
    // Pre-process specific problematic patterns
    let fixed = handle_specific_error_patterns(input);
    
    // Strategy 1: Fix unescaped quotes in string values
    let mut in_string = false;
    let mut escaped = false;
    let mut result = String::new();
    
    for ch in fixed.chars() {
        match ch {
            '"' if !escaped => {
                in_string = !in_string;
                result.push(ch);
            }
            '\\' if !escaped => {
                escaped = true;
                result.push(ch);
            }
            _ => {
                if escaped {
                    escaped = false;
                }
                
                if in_string && ch == '"' && !escaped {
                    // This is an unescaped quote inside a string, escape it
                    result.push('\\');
                }
                result.push(ch);
            }
        }
    }
    
    // Try parsing the fixed version
    if serde_json::from_str::<serde_json::Value>(&result).is_ok() {
        return Some(result);
    }
    
    // Strategy 2: More aggressive fixes
    let mut aggressive_fix = result.clone();
    
    // Remove any trailing commas before closing braces/brackets
    aggressive_fix = aggressive_fix
        .replace(",}", "}")
        .replace(",]", "]")
        .replace(",,", ",");
    
    // Fix common JSON syntax issues
    aggressive_fix = aggressive_fix
        .replace("}{", "},{")  // Fix missing comma between objects
        .replace("][", "],[")  // Fix missing comma between arrays
        .replace("}[", "},["); // Fix missing comma between object and array
    
    if serde_json::from_str::<serde_json::Value>(&aggressive_fix).is_ok() {
        return Some(aggressive_fix);
    }
    
    // Strategy 3: Try to extract valid JSON from the string
    if let Some(extracted) = extract_valid_json_substring(input) {
        return Some(extracted);
    }
    
    // Strategy 4: Last resort - try to create a minimal valid JSON
    if let Some(minimal) = create_minimal_json(input) {
        return Some(minimal);
    }
    
    // If all else fails, return None
    None
}

/// Extract a valid JSON substring from a potentially malformed string
fn extract_valid_json_substring(input: &str) -> Option<String> {
    // Look for JSON object patterns
    if let Some(start) = input.find('{') {
        if let Some(end) = find_matching_brace(&input[start..]) {
            let candidate = &input[start..start + end + 1];
            if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                return Some(candidate.to_string());
            }
        }
    }
    
    // Look for JSON array patterns
    if let Some(start) = input.find('[') {
        if let Some(end) = find_matching_bracket(&input[start..]) {
            let candidate = &input[start..start + end + 1];
            if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                return Some(candidate.to_string());
            }
        }
    }
    
    None
}

/// Find the matching closing brace for an opening brace
fn find_matching_brace(input: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut escaped = false;
    
    for (i, ch) in input.chars().enumerate() {
        match ch {
            '"' if !escaped => {
                in_string = !in_string;
            }
            '\\' if !escaped => {
                escaped = true;
            }
            '{' if !in_string => {
                depth += 1;
            }
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {
                if escaped {
                    escaped = false;
                }
            }
        }
    }
    None
}

/// Find the matching closing bracket for an opening bracket
fn find_matching_bracket(input: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut escaped = false;
    
    for (i, ch) in input.chars().enumerate() {
        match ch {
            '"' if !escaped => {
                in_string = !in_string;
            }
            '\\' if !escaped => {
                escaped = true;
            }
            '[' if !in_string => {
                depth += 1;
            }
            ']' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {
                if escaped {
                    escaped = false;
                }
            }
        }
    }
    None
}

/// Create a minimal valid JSON object from malformed input
fn create_minimal_json(input: &str) -> Option<String> {
    // Try to extract key-value pairs from the malformed JSON
    let mut pairs = Vec::new();
    
    // Look for patterns like "key":"value" or "key":value
    let re = regex::Regex::new(r#""([^"]+)"\s*:\s*("([^"]*)"|([^,}\]]+))"#).ok()?;
    
    for cap in re.captures_iter(input) {
        let key = cap.get(1)?.as_str();
        let value = if let Some(quoted_value) = cap.get(2) {
            quoted_value.as_str()
        } else if let Some(unquoted_value) = cap.get(3) {
            unquoted_value.as_str()
        } else {
            continue;
        };
        
        // Clean up the value
        let clean_value = value.trim().replace("\"", "'");
        pairs.push(format!("\"{}\":\"{}\"", key, clean_value));
    }
    
    if pairs.is_empty() {
        return None;
    }
    
    Some(format!("{{{}}}", pairs.join(",")))
}

/// Enhanced JSON validation with detailed error reporting
fn validate_json_with_details(input: &str) -> Result<serde_json::Value, String> {
    match serde_json::from_str::<serde_json::Value>(input) {
        Ok(value) => Ok(value),
        Err(e) => {
            let error_msg = format!("JSON parse error: {} at line {} column {}", 
                e.to_string(), e.line(), e.column());
            
            // Try to provide more specific error information
            let specific_error = if e.to_string().contains("expected `,` or `}`") {
                "Missing comma or closing brace - likely malformed object structure"
            } else if e.to_string().contains("expected `,` or `]`") {
                "Missing comma or closing bracket - likely malformed array structure"
            } else if e.to_string().contains("expected value") {
                "Missing value - likely trailing comma or incomplete structure"
            } else if e.to_string().contains("expected `\"`") {
                "Missing quote - likely unescaped quote in string"
            } else {
                "Unknown JSON syntax error"
            };
            
            Err(format!("{} - {}", error_msg, specific_error))
        }
    }
}

/// Extract meaningful error information from complex error messages
fn extract_error_info(input: &str) -> String {
    // First handle specific problematic patterns
    let cleaned = handle_specific_error_patterns(input);
    
    // Then apply general cleaning
    let cleaned = clean_error_message(&cleaned);
    
    // Handle specific error patterns
    if cleaned.contains("Status {") || cleaned.contains("Status(") {
        // Extract HTTP status from Status structure
        if let Some(status_start) = cleaned.find("status: ") {
            let status_end = cleaned[status_start..].find(",").unwrap_or(cleaned.len() - status_start);
            let status = &cleaned[status_start + 8..status_start + status_end];
            return format!("HTTP status {}", status);
        }
        return "HTTP error".to_string();
    }
    
    if cleaned.contains("Response {") || cleaned.contains("Response(") {
        // Extract status from Response structure
        if let Some(status_start) = cleaned.find("status: ") {
            let status_end = cleaned[status_start..].find(",").unwrap_or(cleaned.len() - status_start);
            let status = &cleaned[status_start + 8..status_start + status_end];
            return format!("Server returned HTTP status {}", status);
        }
        return "Server response error".to_string();
    }
    
    // Map common error patterns to user-friendly messages
    if cleaned.contains("tls handshake eof") {
        return "TLS handshake failed - server may be offline".to_string();
    }
    
    if cleaned.contains("connection refused") {
        return "Connection refused - server may be offline".to_string();
    }
    
    if cleaned.contains("InvalidContentType") {
        return "Invalid content type - server may not be a valid node".to_string();
    }
    
    if cleaned.contains("timeout") {
        return "Connection timeout".to_string();
    }
    
    if cleaned.contains("dns") {
        return "DNS resolution failed".to_string();
    }
    
    if cleaned.contains("Response body") {
        return "Server returned invalid response".to_string();
    }
    
    // If no specific pattern matches, return a cleaned version
    cleaned
}

fn deserialize_error_field<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    
    match value {
        // Handle null as None
        serde_json::Value::Null => Ok(None),
        
        // Handle boolean values
        serde_json::Value::Bool(b) => {
            if b {
                Ok(Some("Server error occurred".to_string()))
            } else {
                Ok(None)
            }
        },
        
        // Handle direct strings
        serde_json::Value::String(s) => {
            if s.is_empty() {
                return Ok(None);
            }
            
            // Use the improved error message cleaning
            let error_msg = extract_error_info(&s);
            
            if error_msg.is_empty() {
                Ok(None)
            } else {
                Ok(Some(error_msg))
            }
        },
        
        // Handle objects that might contain error messages
        serde_json::Value::Object(obj) => {
            // Try to extract error message from common fields
            let error_msg = obj.get("error")
                .or_else(|| obj.get("message"))
                .or_else(|| obj.get("detail"))
                .and_then(|v| v.as_str())
                .map(|s| extract_error_info(s));
                
            if let Some(msg) = error_msg {
                if msg.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(msg))
                }
            } else {
                // If no error message found, return None
                Ok(None)
            }
        },
        
        // Everything else is treated as None
        _ => Ok(None),
    }
}

fn deserialize_error_message<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
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
            
            // Use the improved error message cleaning
            let error_msg = extract_error_info(&s);
            
            if error_msg.is_empty() {
                Ok(None)
            } else {
                Ok(Some(error_msg))
            }
        },
        
        // Handle objects that might contain error messages
        serde_json::Value::Object(obj) => {
            // Try to extract error message from common fields
            let error_msg = obj.get("error")
                .or_else(|| obj.get("message"))
                .or_else(|| obj.get("detail"))
                .and_then(|v| v.as_str())
                .map(|s| extract_error_info(s));
                
            if let Some(msg) = error_msg {
                if msg.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(msg))
                }
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
            // Try to parse the timestamp with multiple strategies
            let mut parsed_time = None;
            
            // Remove surrounding quotes if present
            let clean_timestamp = last_updated.trim_matches('\'');
            
            // Strategy 0: Use custom parsing function for RFC3339 with nanoseconds
            if let Some(time) = parse_rfc3339_with_nanos(last_updated) {
                parsed_time = Some(time);
            }
            // Strategy 1: Direct RFC3339 parsing
            else if let Ok(time) = DateTime::parse_from_rfc3339(clean_timestamp) {
                parsed_time = Some(time);
            }
            // Strategy 2: Try with Z suffix if missing
            else if let Ok(time) = DateTime::parse_from_rfc3339(&format!("{}Z", clean_timestamp)) {
                parsed_time = Some(time);
            }
            // Strategy 3: Try parsing as naive datetime first (handles nanoseconds better)
            else if clean_timestamp.ends_with('Z') {
                // Remove the Z suffix and parse as naive datetime
                let naive_str = &clean_timestamp[..clean_timestamp.len()-1];
                
                let formats = [
                    "%Y-%m-%dT%H:%M:%S%.f",
                    "%Y-%m-%dT%H:%M:%S%.9f",  // Support for 9-digit nanoseconds
                    "%Y-%m-%dT%H:%M:%S",
                ];
                
                for format in &formats {
                    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(naive_str, format) {
                        parsed_time = Some(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)
                            .with_timezone(&FixedOffset::east_opt(0).unwrap()));
                        break;
                    }
                }
            }
            // Strategy 4: Try naive datetime parsing with nanoseconds
            else {
                let formats = [
                    "%Y-%m-%dT%H:%M:%S%.f",
                    "%Y-%m-%dT%H:%M:%S%.9f",  // Support for 9-digit nanoseconds
                    "%Y-%m-%dT%H:%M:%S",
                    "%Y-%m-%d %H:%M:%S%.f",
                    "%Y-%m-%d %H:%M:%S",
                ];
                
                for format in &formats {
                    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(clean_timestamp, format) {
                        parsed_time = Some(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)
                            .with_timezone(&FixedOffset::east_opt(0).unwrap()));
                        break;
                    }
                }
            }

            if let Some(time) = parsed_time {
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
                // Return a more user-friendly error message
                format!("Invalid time format: {}", last_updated)
            }
        } else {
            "Never".to_string()
        }
    }

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

    fn formatted_uptime_30_day(&self) -> String {
        match self.uptime_30_day {
            Some(uptime) => format!("{:.1}%", uptime),
            None => "-".to_string(),
        }
    }

    fn formatted_version(&self) -> String {
        let lwd_version = self.server_version.as_deref().unwrap_or("-");
        
        // Check if this is a Zaino server by looking at the vendor field
        let is_zaino = self.extra.get("vendor")
            .and_then(|v| v.as_str())
            .map(|v| v.contains("Zaino"))
            .unwrap_or(false);

        let lwd_display = if is_zaino {
            format!("{} (Zaino 🚀)", lwd_version)
        } else {
            lwd_version.to_string()
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
#[template(path = "server.html")]
struct ServerTemplate {
    sorted_data: Vec<(String, Value)>,
    donation_address: String,
    donation_qr_code: String,
    show_donation: bool,
    host: String,
    network: String,
    current_network: &'static str,
    percentile_height: u64,
    online_count: usize,
    total_count: usize,
    uptime_stats: UptimeStats,
    results_window_days: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UptimeStats {
    last_day: f64,
    last_week: f64,
    last_month: f64,
    total_checks: u64,
    last_check: String,
    last_online: String,
    is_currently_online: bool,
    last_day_formatted: String,
    last_week_formatted: String,
    last_month_formatted: String,
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

    // Update query to handle empty results and use FORMAT JSONEachRow, including 30-day uptime
    let query = format!(
        r#"
        WITH latest_results AS (
            SELECT 
                r.*,
                ROW_NUMBER() OVER (PARTITION BY r.hostname ORDER BY r.checked_at DESC) as rn
            FROM {}.results r
            WHERE r.checker_module = '{}'
            AND r.checked_at >= now() - INTERVAL {} DAY
        ),
        uptime_30_day AS (
            SELECT 
                hostname,
                sum(online_count) * 100.0 / sum(total_checks) as uptime_percentage
            FROM {}.uptime_stats
            WHERE time_bucket >= now() - INTERVAL 30 DAY
            GROUP BY hostname
        )
        SELECT 
            lr.hostname,
            lr.checked_at,
            lr.status,
            lr.ping_ms as ping,
            lr.response_data,
            u30.uptime_percentage as uptime_30_day
        FROM latest_results lr
        LEFT JOIN uptime_30_day u30 ON lr.hostname = u30.hostname
        WHERE lr.rn = 1
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database,
        network.0,
        worker.config.results_window_days,
        worker.clickhouse.database
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
        
        // First, try to parse the line as JSON
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
                
                // Validate that response_data looks like valid JSON
                if response_data.trim().is_empty() || response_data == "{}" {
                    warn!("Empty or invalid response_data for host: {}", 
                          result["hostname"].as_str().unwrap_or("unknown"));
                    continue;
                }
                
                // Try to validate and fix the JSON if needed
                let cleaned_response_data = validate_and_fix_json(response_data)
                    .unwrap_or_else(|| {
                        let hostname = result["hostname"].as_str().unwrap_or("unknown");
                        warn!("Could not fix malformed JSON for host: {}", hostname);
                        
                        // Log the problematic JSON for debugging
                        log_problematic_json(hostname, response_data);
                        
                        // Try to get more detailed error information
                        if let Err(detailed_error) = validate_json_with_details(response_data) {
                            warn!("JSON validation details for host {}: {}", hostname, detailed_error);
                        }
                        
                        "{}".to_string()
                    });
                
                // Try to parse the response_data as ServerInfo
                match serde_json::from_str::<ServerInfo>(&cleaned_response_data) {
                    Ok(mut server_info) => {
                        // Add the uptime_30_day from the query result
                        server_info.uptime_30_day = result.get("uptime_30_day")
                            .and_then(|v| v.as_f64());
                        servers.push(server_info);
                    }
                    Err(e) => {
                        // If parsing fails, try to create a minimal ServerInfo with available data
                        warn!(
                            "Failed to parse server info for host {}: {} (raw data: {})", 
                            result["hostname"].as_str().unwrap_or("unknown"),
                            e,
                            response_data
                        );
                        
                        // Log specific field type issues
                        if e.to_string().contains("invalid type") {
                            warn!("Field type mismatch detected. This usually means a field is stored as a string when it should be a number, or vice versa.");
                            
                            // Try to identify which field has the type issue
                            if e.to_string().contains("expected u64") {
                                warn!("Height field type issue detected - height should be a number, not a string");
                            }
                            if e.to_string().contains("expected f64") {
                                warn!("Ping field type issue detected - ping should be a number, not a string");
                            }
                            if e.to_string().contains("expected u16") {
                                warn!("Port field type issue detected - port should be a number, not a string");
                            }
                            if e.to_string().contains("expected a boolean") {
                                warn!("Boolean field type issue detected - field should be a boolean, not a string");
                            }
                        }
                        
                        // Create a fallback ServerInfo with basic information
                        if let Some(hostname) = result["hostname"].as_str() {
                            let mut fallback_server = ServerInfo {
                                host: hostname.to_string(),
                                port: None,
                                height: 0,
                                status: "error".to_string(),
                                error: Some("Failed to parse server response".to_string()),
                                error_type: Some("parse_error".to_string()),
                                error_message: Some("Server response could not be parsed".to_string()),
                                last_updated: result["checked_at"].as_str().map(|s| s.to_string()),
                                ping: result["ping"].as_f64(),
                                server_version: None,
                                user_submitted: false,
                                check_id: None,
                                extra: HashMap::new(),
                                uptime_30_day: result.get("uptime_30_day").and_then(|v| v.as_f64()),
                            };
                            
                            // Try to extract basic information from the raw response_data
                            if let Ok(raw_value) = serde_json::from_str::<serde_json::Value>(&cleaned_response_data) {
                                if let Some(obj) = raw_value.as_object() {
                                    // Extract height if available
                                    if let Some(height_val) = obj.get("height") {
                                        if let Some(height) = height_val.as_u64() {
                                            fallback_server.height = height;
                                        }
                                    }
                                    
                                    // Extract status if available
                                    if let Some(status_val) = obj.get("status") {
                                        if let Some(status) = status_val.as_str() {
                                            fallback_server.status = status.to_string();
                                        }
                                    }
                                    
                                    // Extract server_version if available
                                    if let Some(version_val) = obj.get("server_version") {
                                        if let Some(version) = version_val.as_str() {
                                            fallback_server.server_version = Some(version.to_string());
                                        }
                                    }
                                    
                                    // Extract error information if available
                                    if let Some(error_val) = obj.get("error") {
                                        if let Some(error) = error_val.as_str() {
                                            fallback_server.error = Some(extract_error_info(error));
                                        }
                                    }
                                    
                                    if let Some(error_type_val) = obj.get("error_type") {
                                        if let Some(error_type) = error_type_val.as_str() {
                                            fallback_server.error_type = Some(error_type.to_string());
                                        }
                                    }
                                    
                                    if let Some(error_msg_val) = obj.get("error_message") {
                                        if let Some(error_msg) = error_msg_val.as_str() {
                                            fallback_server.error_message = Some(extract_error_info(error_msg));
                                        }
                                    }
                                    
                                    // Extract port if available
                                    if let Some(port_val) = obj.get("port") {
                                        if let Some(port) = port_val.as_u64() {
                                            fallback_server.port = u16::try_from(port).ok();
                                        }
                                    }
                                    
                                    // Store any additional fields in extra
                                    for (key, value) in obj {
                                        if !["host", "port", "height", "status", "error", "error_type", 
                                             "error_message", "last_updated", "ping", "server_version", 
                                             "user_submitted", "check_id"].contains(&key.as_str()) {
                                            fallback_server.extra.insert(key.clone(), value.clone());
                                        }
                                    }
                                }
                            }
                            
                            servers.push(fallback_server);
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to parse JSON line: {} (raw line: {})", e, line);
                
                // Try to extract at least the hostname from the malformed line
                if let Some(hostname_start) = line.find("\"hostname\":\"") {
                    let start = hostname_start + 12; // Skip "hostname":"
                    if let Some(hostname_end) = line[start..].find("\"") {
                        let hostname = &line[start..start + hostname_end];
                        
                        let fallback_server = ServerInfo {
                            host: hostname.to_string(),
                            port: None,
                            height: 0,
                            status: "error".to_string(),
                            error: Some("Malformed JSON response".to_string()),
                            error_type: Some("parse_error".to_string()),
                            error_message: Some("Server response contains invalid JSON".to_string()),
                            last_updated: None,
                            ping: None,
                            server_version: None,
                            user_submitted: false,
                            check_id: None,
                            extra: HashMap::new(),
                            uptime_30_day: None,
                        };
                        
                        servers.push(fallback_server);
                    }
                }
            }
        }
    }

    // Sort servers: online first, then by ping (descending), offline servers by hostname
    servers.sort_by(|a, b| {
        match (a.is_online(), b.is_online()) {
            (true, true) => {
                // Both online, sort by ping (ascending) then hostname
                match (a.ping, b.ping) {
                    (Some(ping_a), Some(ping_b)) => {
                        // Both have ping values, sort by ping ascending (lowest first)
                        ping_a.partial_cmp(&ping_b).unwrap_or(std::cmp::Ordering::Equal)
                            .then(a.host.to_lowercase().cmp(&b.host.to_lowercase()))
                    },
                    (Some(_), None) => std::cmp::Ordering::Less,  // a has ping, b doesn't
                    (None, Some(_)) => std::cmp::Ordering::Greater,  // b has ping, a doesn't
                    (None, None) => {
                        // Neither has ping, sort by hostname
                        a.host.to_lowercase().cmp(&b.host.to_lowercase())
                    }
                }
            },
            (true, false) => std::cmp::Ordering::Less,  // a online, b offline
            (false, true) => std::cmp::Ordering::Greater,  // b online, a offline
            (false, false) => {
                // Both offline, sort by hostname
                a.host.to_lowercase().cmp(&b.host.to_lowercase())
            }
        }
    });

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

    // Calculate uptime statistics
    let uptime_stats = calculate_uptime_stats(&worker, &host, &network).await?;

    // Create sorted data for alphabetical display
    let mut sorted_data: Vec<(String, Value)> = data.iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    sorted_data.sort_by(|a, b| a.0.cmp(&b.0));

    // Extract donation_address if it exists
    let donation_opt = data.get("donation_address")
        .and_then(|v| v.as_str());
    let show_donation = donation_opt.is_some();
    let donation_address = donation_opt.unwrap_or("").to_string();

    // Generate QR code SVG for donation address
    let donation_qr_code = if show_donation && !donation_address.is_empty() {
        match QrCode::new(&donation_address) {
            Ok(code) => {
                code.render()
                    .min_dimensions(200, 200)
                    .dark_color(svg::Color("#000000"))
                    .light_color(svg::Color("#FFFFFF"))
                    .build()
            },
            Err(_) => String::new()
        }
    } else {
        String::new()
    };

    let template = ServerTemplate {
        sorted_data,
        donation_address,
        donation_qr_code,
        show_donation,
        host,
        network,
        current_network: safe_network.0,
        percentile_height,
        online_count,
        total_count,
        uptime_stats,
        results_window_days: worker.config.results_window_days,
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

        servers.sort_by(|a, b| {
            match (a.is_online(), b.is_online()) {
                (true, true) => {
                    // Both online, sort by ping (ascending) then hostname
                    match (a.ping, b.ping) {
                        (Some(ping_a), Some(ping_b)) => {
                            // Both have ping values, sort by ping ascending (lowest first)
                            ping_a.partial_cmp(&ping_b).unwrap_or(std::cmp::Ordering::Equal)
                                .then(a.host.to_lowercase().cmp(&b.host.to_lowercase()))
                        },
                        (Some(_), None) => std::cmp::Ordering::Less,  // a has ping, b doesn't
                        (None, Some(_)) => std::cmp::Ordering::Greater,  // b has ping, a doesn't
                        (None, None) => {
                            // Neither has ping, sort by hostname
                            a.host.to_lowercase().cmp(&b.host.to_lowercase())
                        }
                    }
                },
                (true, false) => std::cmp::Ordering::Less,  // a online, b offline
                (false, true) => std::cmp::Ordering::Greater,  // b online, a offline
                (false, false) => {
                    // Both offline, sort by hostname
                    a.host.to_lowercase().cmp(&b.host.to_lowercase())
                }
            }
        });

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

async fn calculate_uptime_stats(
    worker: &Worker,
    host: &str,
    _network: &str,
) -> Result<UptimeStats, actix_web::Error> {
    // Query for uptime statistics using the uptime_stats materialized view
    let uptime_query = format!(
        r#"
        SELECT 
            'day' as period,
            sum(online_count) * 100.0 / sum(total_checks) as uptime_percentage
        FROM {}.uptime_stats
        WHERE hostname = '{}'
        AND time_bucket >= now() - INTERVAL 1 DAY
        
        UNION ALL
        
        SELECT 
            'week' as period,
            sum(online_count) * 100.0 / sum(total_checks) as uptime_percentage
        FROM {}.uptime_stats
        WHERE hostname = '{}'
        AND time_bucket >= now() - INTERVAL 7 DAY
        
        UNION ALL
        
        SELECT 
            'month' as period,
            sum(online_count) * 100.0 / sum(total_checks) as uptime_percentage
        FROM {}.uptime_stats
        WHERE hostname = '{}'
        AND time_bucket >= now() - INTERVAL 30 DAY
        
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database, host,
        worker.clickhouse.database, host,
        worker.clickhouse.database, host
    );

    let response = worker.http_client.post(&worker.clickhouse.url)
        .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
        .header("Content-Type", "text/plain")
        .body(uptime_query)
        .send()
        .await
        .map_err(|e| {
            error!("ClickHouse uptime query error: {}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    let status = response.status();
    let body = response.text().await.map_err(|e| {
        error!("Failed to read uptime response body: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to read database response")
    })?;

    if !status.is_success() {
        error!("ClickHouse uptime query failed with status {}: {}", status, body);
        return Err(actix_web::error::ErrorInternalServerError("Database query failed"));
    }

    // Parse the uptime statistics
    let mut last_day = 0.0;
    let mut last_week = 0.0;
    let mut last_month = 0.0;

    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(result) = serde_json::from_str::<serde_json::Value>(line) {
            if let (Some(period), Some(uptime)) = (
                result["period"].as_str(),
                result["uptime_percentage"].as_f64()
            ) {
                match period {
                    "day" => last_day = uptime,
                    "week" => last_week = uptime,
                    "month" => last_month = uptime,
                    _ => {}
                }
            }
        }
    }

    // Get total checks, last check time, last online time, and current status
    let stats_query = format!(
        r#"
        WITH latest_check AS (
            SELECT status, checked_at
            FROM {}.results
            WHERE hostname = '{}'
            ORDER BY checked_at DESC
            LIMIT 1
        )
        SELECT 
            count(*) as total_checks,
            max(checked_at) as last_check,
            max(CASE WHEN status = 'online' THEN checked_at END) as last_online,
            (SELECT status FROM latest_check) as current_status
        FROM {}.results
        WHERE hostname = '{}'
        AND checked_at >= now() - INTERVAL {} DAY
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database, host, worker.clickhouse.database, host, worker.config.results_window_days
    );

    // info!("🔍 Stats query for host {}: {}", host, stats_query.replace("\n", " "));

    // Also run a debug query to see what data exists for this host
    let debug_query = format!(
        r#"
        SELECT 
            hostname,
            checker_module,
            count(*) as check_count,
            max(checked_at) as last_check
        FROM {}.results
        WHERE hostname = '{}'
        GROUP BY hostname, checker_module
        ORDER BY check_count DESC
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database, host
    );

    let debug_response = worker.http_client.post(&worker.clickhouse.url)
        .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
        .header("Content-Type", "text/plain")
        .body(debug_query)
        .send()
        .await;

    if let Ok(debug_resp) = debug_response {
        if let Ok(_debug_body) = debug_resp.text().await {
            // Debug response logged but not used
        }
    }

    let stats_response = worker.http_client.post(&worker.clickhouse.url)
        .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
        .header("Content-Type", "text/plain")
        .body(stats_query)
        .send()
        .await
        .map_err(|e| {
            error!("ClickHouse stats query error: {}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    let _status = stats_response.status();
    let stats_body = stats_response.text().await.map_err(|e| {
        error!("Failed to read stats response body: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to read database response")
    })?;

    let mut total_checks = 0u64;
    let mut last_check = String::new();
    let mut last_online = String::new();
    let mut is_currently_online = false;

    for line in stats_body.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(result) = serde_json::from_str::<serde_json::Value>(line) {
            // Handle total_checks - it might be a string or number
            if let Some(checks) = result["total_checks"].as_u64() {
                total_checks = checks;
            } else if let Some(checks_str) = result["total_checks"].as_str() {
                if let Ok(checks) = checks_str.parse::<u64>() {
                    total_checks = checks;
                }
            }
            
            if let Some(check_time) = result["last_check"].as_str() {
                last_check = check_time.to_string();
            }
            
            // Handle last_online - could be a string or NULL
            if let Some(online_time) = result["last_online"].as_str() {
                last_online = online_time.to_string();
            } else {
                // If last_online is NULL or missing, set to empty string
                last_online = String::new();
            }
            
            if let Some(current_status) = result["current_status"].as_str() {
                is_currently_online = current_status == "online";
            }
            
            // Debug logging for this specific server
            if host == "lightwalletd.stakehold.rs" {
                info!("🔍 Debug for {}: current_status={:?}, is_currently_online={}, last_online='{}'", 
                      host, result["current_status"], is_currently_online, last_online);
            }
        }
    }

    Ok(UptimeStats {
        last_day,
        last_week,
        last_month,
        total_checks,
        last_check,
        last_online,
        is_currently_online,
        last_day_formatted: format!("{:.1}%", last_day),
        last_week_formatted: format!("{:.1}%", last_week),
        last_month_formatted: format!("{:.1}%", last_month),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_clean_error_message() {
        // Test basic cleaning
        let input = "Failed to query server: Response { status: 400, version: HTTP/1.1, headers: {\"content-type\": \"application/json\"}, body: UnsyncBoxBody }";
        let cleaned = clean_error_message(input);
        assert!(!cleaned.contains("\""));
        assert!(!cleaned.contains("{"));
        assert!(!cleaned.contains("}"));
        assert!(cleaned.contains("400"));
    }
    
    #[test]
    fn test_extract_error_info() {
        // Test HTTP status extraction
        let input = "Failed to query server: Response { status: 400, version: HTTP/1.1, headers: {\"content-type\": \"application/json\"}, body: UnsyncBoxBody }";
        let result = extract_error_info(input);
        assert_eq!(result, "Server returned HTTP status 400");
        
        // Test TLS error
        let input = "tls handshake eof";
        let result = extract_error_info(input);
        assert_eq!(result, "TLS handshake failed - server may be offline");
        
        // Test connection refused
        let input = "connection refused";
        let result = extract_error_info(input);
        assert_eq!(result, "Connection refused - server may be offline");
    }
    
    #[test]
    fn test_validate_and_fix_json() {
        // Test valid JSON
        let input = r#"{"host":"test.com","port":50002,"height":0}"#;
        let result = validate_and_fix_json(input);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), input);
        
        // Test JSON with unescaped quotes
        let input = r#"{"host":"test.com","error_message":"Failed to query server: Response { status: 400, headers: {"content-type": "application/json"} }"}"#;
        let result = validate_and_fix_json(input);
        assert!(result.is_some());
        
        // Test invalid JSON
        let input = r#"{"host":"test.com","port":50002,}"#;
        let result = validate_and_fix_json(input);
        assert!(result.is_some()); // Should be fixed by removing trailing comma
        
        // Test JSON with missing commas
        let input = r#"{"host":"test.com" "port":50002}"#;
        let result = validate_and_fix_json(input);
        assert!(result.is_some());
        
        // Test JSON with malformed structure
        let input = r#"{"host":"test.com","error_message":"Response { status: 400, body: UnsyncBoxBody }"}"#;
        let result = validate_and_fix_json(input);
        assert!(result.is_some());
    }
    
    #[test]
    fn test_extract_valid_json_substring() {
        // Test extracting valid JSON from malformed string
        let input = r#"some text {"host":"test.com","port":50002} more text"#;
        let result = extract_valid_json_substring(input);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), r#"{"host":"test.com","port":50002}"#);
        
        // Test with nested objects
        let input = r#"{"outer":{"inner":"value"}}"#;
        let result = extract_valid_json_substring(input);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), input);
    }
    
    #[test]
    fn test_create_minimal_json() {
        // Test creating minimal JSON from malformed input
        let input = r#"{"host":"test.com" "port":50002 "error":"some error"}"#;
        let result = create_minimal_json(input);
        assert!(result.is_some());
        
        // Test with quoted values
        let input = r#"{"host":"test.com","error_message":"Response { status: 400 }"}"#;
        let result = create_minimal_json(input);
        assert!(result.is_some());
    }
    
    #[test]
    fn test_validate_json_with_details() {
        // Test valid JSON
        let input = r#"{"host":"test.com","port":50002}"#;
        let result = validate_json_with_details(input);
        assert!(result.is_ok());
        
        // Test invalid JSON
        let input = r#"{"host":"test.com","port":50002,}"#;
        let result = validate_json_with_details(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected value"));
        
        // Test JSON with unescaped quotes
        let input = r#"{"host":"test.com","error":"Response { status: 400 }"}"#;
        let result = validate_json_with_details(input);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_handle_specific_error_patterns() {
        // Test UnsyncBoxBody replacement
        let input = r#"{"error_message":"Failed to query server: Response { status: 400, body: UnsyncBoxBody }"}"#;
        let result = handle_specific_error_patterns(input);
        assert!(result.contains("Response body"));
        assert!(!result.contains("UnsyncBoxBody"));
        
        // Test Response structure handling
        let input = r#"{"error_message":"Response { status: 400, headers: {"content-type": "application/json"} }"}"#;
        let result = handle_specific_error_patterns(input);
        assert!(result.contains("Response("));
        assert!(result.contains("headers: ("));
    }
    
    #[test]
    fn test_deserialize_height() {
        // Test number height
        let json = r#"{"height":12345}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let height = deserialize_height(serde::Deserializer::from(serde_json::to_value(result["height"]).unwrap())).unwrap();
        assert_eq!(height, 12345);
        
        // Test string height
        let json = r#"{"height":"0"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let height = deserialize_height(serde::Deserializer::from(serde_json::to_value(result["height"]).unwrap())).unwrap();
        assert_eq!(height, 0);
        
        // Test empty string height
        let json = r#"{"height":""}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let height = deserialize_height(serde::Deserializer::from(serde_json::to_value(result["height"]).unwrap())).unwrap();
        assert_eq!(height, 0);
        
        // Test null height
        let json = r#"{"height":null}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let height = deserialize_height(serde::Deserializer::from(serde_json::to_value(result["height"]).unwrap())).unwrap();
        assert_eq!(height, 0);
    }
    
    #[test]
    fn test_deserialize_ping() {
        // Test number ping
        let json = r#"{"ping":123.45}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let ping = deserialize_ping(serde::Deserializer::from(serde_json::to_value(result["ping"]).unwrap())).unwrap();
        assert_eq!(ping, Some(123.45));
        
        // Test string ping
        let json = r#"{"ping":"123.45"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let ping = deserialize_ping(serde::Deserializer::from(serde_json::to_value(result["ping"]).unwrap())).unwrap();
        assert_eq!(ping, Some(123.45));
        
        // Test empty string ping
        let json = r#"{"ping":""}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let ping = deserialize_ping(serde::Deserializer::from(serde_json::to_value(result["ping"]).unwrap())).unwrap();
        assert_eq!(ping, None);
        
        // Test null ping
        let json = r#"{"ping":null}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let ping = deserialize_ping(serde::Deserializer::from(serde_json::to_value(result["ping"]).unwrap())).unwrap();
        assert_eq!(ping, None);
    }
    
    #[test]
    fn test_deserialize_user_submitted() {
        // Test boolean true
        let json = r#"{"user_submitted":true}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(serde_json::to_value(result["user_submitted"]).unwrap())).unwrap();
        assert_eq!(user_submitted, true);
        
        // Test boolean false
        let json = r#"{"user_submitted":false}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(serde_json::to_value(result["user_submitted"]).unwrap())).unwrap();
        assert_eq!(user_submitted, false);
        
        // Test string "true"
        let json = r#"{"user_submitted":"true"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(serde_json::to_value(result["user_submitted"]).unwrap())).unwrap();
        assert_eq!(user_submitted, true);
        
        // Test string "false"
        let json = r#"{"user_submitted":"false"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(serde_json::to_value(result["user_submitted"]).unwrap())).unwrap();
        assert_eq!(user_submitted, false);
        
        // Test string "FALSE" (case insensitive)
        let json = r#"{"user_submitted":"FALSE"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(serde_json::to_value(result["user_submitted"]).unwrap())).unwrap();
        assert_eq!(user_submitted, false);
        
        // Test number 1 (true)
        let json = r#"{"user_submitted":1}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(serde_json::to_value(result["user_submitted"]).unwrap())).unwrap();
        assert_eq!(user_submitted, true);
        
        // Test number 0 (false)
        let json = r#"{"user_submitted":0}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(serde_json::to_value(result["user_submitted"]).unwrap())).unwrap();
        assert_eq!(user_submitted, false);
        
        // Test null (defaults to false)
        let json = r#"{"user_submitted":null}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(serde_json::to_value(result["user_submitted"]).unwrap())).unwrap();
        assert_eq!(user_submitted, false);
    }
    
    #[test]
    fn test_deserialize_error_field() {
        // Test boolean true
        let json = r#"{"error":true}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let error = deserialize_error_field(serde::Deserializer::from(serde_json::to_value(result["error"]).unwrap())).unwrap();
        assert_eq!(error, Some("Server error occurred".to_string()));
        
        // Test boolean false
        let json = r#"{"error":false}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let error = deserialize_error_field(serde::Deserializer::from(serde_json::to_value(result["error"]).unwrap())).unwrap();
        assert_eq!(error, None);
        
        // Test string error
        let json = r#"{"error":"Connection failed"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let error = deserialize_error_field(serde::Deserializer::from(serde_json::to_value(result["error"]).unwrap())).unwrap();
        assert_eq!(error, Some("Connection failed"));
        
        // Test null
        let json = r#"{"error":null}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let error = deserialize_error_field(serde::Deserializer::from(serde_json::to_value(result["error"]).unwrap())).unwrap();
        assert_eq!(error, None);
    }
    
    #[test]
    fn test_server_info_with_problematic_json() {
        // Test with the exact JSON format from the error logs
        let json = r#"{"host":"128.0.190.26","port":50002,"height":"0","server_version":"unknown","last_updated":"2025-07-31T21:11:21.472525544Z","error":true,"error_type":"connection_error","error_message":"Failed to query server: Response { status: 400, version: HTTP/1.1, headers: {\"content-type\": \"application/json\"}, body: UnsyncBoxBody }","user_submitted":"false","check_id":"539cb1f6-1855-5045-bb27-215221a4be25","status":"error"}"#;
        
        let server_info: ServerInfo = serde_json::from_str(json).unwrap();
        assert_eq!(server_info.host, "128.0.190.26");
        assert_eq!(server_info.port, Some(50002));
        assert_eq!(server_info.height, 0);
        assert_eq!(server_info.status, "error");
        assert!(server_info.error.is_some());
        assert_eq!(server_info.error_type, Some("connection_error".to_string()));
        assert!(server_info.error_message.is_some());
        assert_eq!(server_info.user_submitted, false);
    }
    
    #[test]
    fn test_timestamp_parsing() {
        // Test RFC3339 timestamp parsing
        let server_info = ServerInfo {
            host: "test.com".to_string(),
            port: Some(50002),
            height: 0,
            status: "error".to_string(),
            error: Some("test error".to_string()),
            error_type: Some("connection_error".to_string()),
            error_message: Some("test message".to_string()),
            ping: None,
            server_version: Some("unknown".to_string()),
            user_submitted: false,
            check_id: Some("test-id".to_string()),
            extra: HashMap::new(),
            last_updated: Some("2025-07-31T21:11:21.472525544Z".to_string()),
        };
        
        let formatted = server_info.formatted_last_updated();
        // Should not contain "Invalid time format"
        assert!(!formatted.contains("Invalid time format"));
        // Should contain some time information
        assert!(formatted.len() > 0);
        
        // Test with the exact timestamp from the logs
        let server_info2 = ServerInfo {
            host: "128.0.190.26".to_string(),
            port: Some(50002),
            height: 0,
            status: "error".to_string(),
            error: Some("test error".to_string()),
            error_type: Some("connection_error".to_string()),
            error_message: Some("test message".to_string()),
            ping: None,
            server_version: Some("unknown".to_string()),
            user_submitted: false,
            check_id: Some("test-id".to_string()),
            extra: HashMap::new(),
            last_updated: Some("2025-07-31T21:11:21.472525544Z".to_string()),
        };
        
        let formatted2 = server_info2.formatted_last_updated();
        assert!(!formatted2.contains("Invalid time format"));
        assert!(formatted2.len() > 0);
    }
    
    #[test]
    fn test_parse_rfc3339_with_nanos() {
        // Test the custom parsing function
        let timestamp = "2025-07-31T21:11:21.472525544Z";
        let parsed = parse_rfc3339_with_nanos(timestamp);
        assert!(parsed.is_some());
        
        // Test with quoted timestamp
        let timestamp_quoted = "'2025-07-31T21:11:21.472525544Z'";
        let parsed_quoted = parse_rfc3339_with_nanos(timestamp_quoted);
        assert!(parsed_quoted.is_some());
        
        // Test with different nanosecond formats
        let timestamp2 = "2025-07-31T21:11:21.123456789Z";
        let parsed2 = parse_rfc3339_with_nanos(timestamp2);
        assert!(parsed2.is_some());
        
        // Test with standard RFC3339 format
        let timestamp3 = "2025-07-31T21:11:21Z";
        let parsed3 = parse_rfc3339_with_nanos(timestamp3);
        assert!(parsed3.is_some());
    }
    
    #[test]
    fn test_deserialize_host() {
        // Test quoted hostname
        let json = r#"{"host":"'128.0.190.26'"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let host = deserialize_host(serde::Deserializer::from(serde_json::to_value(result["host"]).unwrap())).unwrap();
        assert_eq!(host, "128.0.190.26");
        
        // Test unquoted hostname
        let json = r#"{"host":"example.com"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let host = deserialize_host(serde::Deserializer::from(serde_json::to_value(result["host"]).unwrap())).unwrap();
        assert_eq!(host, "example.com");
    }
    
    #[test]
    fn test_deserialize_server_version() {
        // Test quoted server version
        let json = r#"{"server_version":"'unknown'"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let version = deserialize_server_version(serde::Deserializer::from(serde_json::to_value(result["server_version"]).unwrap())).unwrap();
        assert_eq!(version, Some("unknown".to_string()));
        
        // Test unquoted server version
        let json = r#"{"server_version":"ElectrumX 1.16.0"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let version = deserialize_server_version(serde::Deserializer::from(serde_json::to_value(result["server_version"]).unwrap())).unwrap();
        assert_eq!(version, Some("ElectrumX 1.16.0".to_string()));
        
        // Test null server version
        let json = r#"{"server_version":null}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let version = deserialize_server_version(serde::Deserializer::from(serde_json::to_value(result["server_version"]).unwrap())).unwrap();
        assert_eq!(version, None);
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize tracing subscriber with environment filter
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false)
        .with_ansi(true)
        .pretty();
    subscriber.init();
    
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

/// Log problematic JSON data for debugging
fn log_problematic_json(hostname: &str, json_data: &str) {
    // Truncate long JSON for logging
    let truncated = if json_data.len() > 500 {
        format!("{}...", &json_data[..500])
    } else {
        json_data.to_string()
    };
    
    warn!("Problematic JSON for host {}: {}", hostname, truncated);
    
    // Try to identify the specific issue
    if json_data.contains("expected `,` or `}`") {
        warn!("Issue: Missing comma or closing brace in JSON structure");
    } else if json_data.contains("expected `\"`") {
        warn!("Issue: Unescaped quotes in JSON string");
    } else if json_data.contains("expected value") {
        warn!("Issue: Missing value or trailing comma");
    } else if json_data.contains("UnsyncBoxBody") {
        warn!("Issue: Contains unescaped response body text");
    }
}

/// Handle specific problematic patterns in error messages
fn handle_specific_error_patterns(input: &str) -> String {
    let mut cleaned = input.to_string();
    
    // Handle UnsyncBoxBody pattern specifically
    if cleaned.contains("UnsyncBoxBody") {
        cleaned = cleaned.replace("UnsyncBoxBody", "Response body");
    }
    
    // Handle other common problematic patterns
    cleaned = cleaned
        .replace("Response {", "Response(")
        .replace("Status {", "Status(")
        .replace("headers: {", "headers: (")
        .replace("body: {", "body: (")
        .replace("},", "),")
        .replace("}", ")");
    
    cleaned
}

/// Custom function to parse RFC3339 timestamps with nanoseconds
fn parse_rfc3339_with_nanos(timestamp: &str) -> Option<DateTime<FixedOffset>> {
    // Remove surrounding quotes if present
    let clean_timestamp = timestamp.trim_matches('\'');
    
    // Handle the specific format: 2025-07-31T21:11:21.472525544Z
    if clean_timestamp.ends_with('Z') {
        let naive_str = &clean_timestamp[..clean_timestamp.len()-1];
        
        // Try parsing with different nanosecond formats
        let formats = [
            "%Y-%m-%dT%H:%M:%S%.f",
            "%Y-%m-%dT%H:%M:%S%.9f",
            "%Y-%m-%dT%H:%M:%S%.6f",
            "%Y-%m-%dT%H:%M:%S%.3f",
        ];
        
        for format in &formats {
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(naive_str, format) {
                return Some(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)
                    .with_timezone(&FixedOffset::east_opt(0).unwrap()));
            }
        }
    }
    
    // Fallback to standard RFC3339 parsing
    DateTime::parse_from_rfc3339(clean_timestamp).ok()
}

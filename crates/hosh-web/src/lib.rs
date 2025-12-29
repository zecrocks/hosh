//! Hosh Web - HTTP dashboard and API server for the Hosh monitoring system.
//!
//! This crate provides the web interface for viewing server status,
//! as well as the API endpoints for checkers to submit results.

use actix_files as fs;
use actix_web::{
    get,
    middleware::Logger,
    post,
    web::{self, Redirect},
    App, HttpResponse, HttpServer, Result,
};
use askama::Template;
use chrono::{DateTime, FixedOffset, Utc};
use qrcode::{render::svg, QrCode};
use serde::de::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

// =============================================================================
// LEADERBOARD VERSION REQUIREMENTS
// Only servers running these versions (or newer) are included in the leaderboard
// =============================================================================
const LEADERBOARD_MIN_ZEBRA_VERSION: &str = "3.1.0";
const LEADERBOARD_MIN_LWD_VERSION: &str = "0.4.18";
const LEADERBOARD_MIN_ZAINO_VERSION: &str = "0.1.2";

mod filters {
    use askama::Result;
    use serde_json::Value;

    pub fn format_value(v: &Value) -> Result<String> {
        match v {
            Value::String(s) => Ok(s.to_string()),
            Value::Number(n) => Ok(n.to_string()),
            Value::Bool(b) => Ok(b.to_string()),
            Value::Null => Ok("null".to_string()),
            _ => Ok(v.to_string()),
        }
    }
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    servers: Vec<ServerInfo>,
    percentile_height: u64,
    current_network: &'static str,
    total_count: usize,
    community_count: usize,
    hide_community: bool,
    tor_only: bool,
    onion_count: usize,
}

#[derive(Clone)]
struct LeaderboardEntry {
    rank: usize,
    server: ServerInfo,
}

#[derive(Template)]
#[template(path = "leaderboard.html")]
struct LeaderboardTemplate {
    entries: Vec<LeaderboardEntry>,
    current_network: &'static str,
    percentile_height: u64,
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

    #[serde(default, deserialize_with = "deserialize_community")]
    community: bool,

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
        Value::Number(n) => n
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .map(Some)
            .or(Some(None))
            .ok_or_else(|| D::Error::custom("Invalid port number")),
        Value::String(s) => {
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<u16>().map(Some).or(Ok(None))
            }
        }
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
        }
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
        Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| D::Error::custom("Invalid height number")),
        Value::String(s) => {
            if s.is_empty() {
                Ok(0)
            } else {
                s.parse::<u64>()
                    .map_err(|_| D::Error::custom("Failed to parse height string as number"))
            }
        }
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
        Value::Number(n) => n
            .as_f64()
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
        }
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
        }
        Value::Number(n) => {
            if let Some(num) = n.as_u64() {
                Ok(num != 0)
            } else {
                warn!("Unexpected user_submitted number value: {:?}", n);
                Ok(false)
            }
        }
        Value::Null => Ok(false),
        _ => {
            warn!("Unexpected user_submitted value format: {:?}", value);
            Ok(false)
        }
    }
}

fn deserialize_community<'de, D>(deserializer: D) -> Result<bool, D::Error>
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
                    warn!("Unexpected community string value: {:?}", s);
                    Ok(false) // Default to false for unknown values
                }
            }
        }
        Value::Number(n) => {
            if let Some(num) = n.as_u64() {
                Ok(num != 0)
            } else {
                warn!("Unexpected community number value: {:?}", n);
                Ok(false)
            }
        }
        Value::Null => Ok(false),
        _ => {
            warn!("Unexpected community value format: {:?}", value);
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
        }
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
        .replace("\"", "'") // Replace unescaped quotes with single quotes
        .replace("{", "(") // Replace unescaped braces with parentheses
        .replace("}", ")")
        .replace("[", "(") // Replace unescaped brackets with parentheses
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
        .replace("}{", "},{") // Fix missing comma between objects
        .replace("][", "],[") // Fix missing comma between arrays
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
            let error_msg = format!(
                "JSON parse error: {} at line {} column {}",
                e,
                e.line(),
                e.column()
            );

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
            let status_end = cleaned[status_start..]
                .find(",")
                .unwrap_or(cleaned.len() - status_start);
            let status = &cleaned[status_start + 8..status_start + status_end];
            return format!("HTTP status {}", status);
        }
        return "HTTP error".to_string();
    }

    if cleaned.contains("Response {") || cleaned.contains("Response(") {
        // Extract status from Response structure
        if let Some(status_start) = cleaned.find("status: ") {
            let status_end = cleaned[status_start..]
                .find(",")
                .unwrap_or(cleaned.len() - status_start);
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
        }

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
        }

        // Handle objects that might contain error messages
        serde_json::Value::Object(obj) => {
            // Try to extract error message from common fields
            let error_msg = obj
                .get("error")
                .or_else(|| obj.get("message"))
                .or_else(|| obj.get("detail"))
                .and_then(|v| v.as_str())
                .map(extract_error_info);

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
        }

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
        }

        // Handle objects that might contain error messages
        serde_json::Value::Object(obj) => {
            // Try to extract error message from common fields
            let error_msg = obj
                .get("error")
                .or_else(|| obj.get("message"))
                .or_else(|| obj.get("detail"))
                .and_then(|v| v.as_str())
                .map(extract_error_info);

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
        }

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
            else if let Ok(time) = DateTime::parse_from_rfc3339(&format!("{}Z", clean_timestamp))
            {
                parsed_time = Some(time);
            }
            // Strategy 3: Try parsing as naive datetime first (handles nanoseconds better)
            else if let Some(naive_str) = clean_timestamp.strip_suffix('Z') {
                // Remove the Z suffix and parse as naive datetime
                let formats = [
                    "%Y-%m-%dT%H:%M:%S%.f",
                    "%Y-%m-%dT%H:%M:%S%.9f", // Support for 9-digit nanoseconds
                    "%Y-%m-%dT%H:%M:%S",
                ];

                for format in &formats {
                    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(naive_str, format) {
                        parsed_time = Some(
                            DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)
                                .with_timezone(&FixedOffset::east_opt(0).unwrap()),
                        );
                        break;
                    }
                }
            }
            // Strategy 4: Try naive datetime parsing with nanoseconds
            else {
                let formats = [
                    "%Y-%m-%dT%H:%M:%S%.f",
                    "%Y-%m-%dT%H:%M:%S%.9f", // Support for 9-digit nanoseconds
                    "%Y-%m-%dT%H:%M:%S",
                    "%Y-%m-%d %H:%M:%S%.f",
                    "%Y-%m-%d %H:%M:%S",
                ];

                for format in &formats {
                    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(clean_timestamp, format) {
                        parsed_time = Some(
                            DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)
                                .with_timezone(&FixedOffset::east_opt(0).unwrap()),
                        );
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
            Some(uptime) => format!("{:.2}%", uptime),
            None => "-".to_string(),
        }
    }

    fn formatted_version(&self) -> String {
        let lwd_version = self.server_version.as_deref().unwrap_or("-");

        // Check if this is a Zaino server by looking at the vendor field
        let is_zaino = self
            .extra
            .get("vendor")
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

    fn is_community(&self) -> bool {
        self.community
    }

    fn has_donation_address(&self) -> bool {
        self.extra
            .get("donation_address")
            .and_then(|v| v.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
    }

    fn is_testnet(&self) -> bool {
        self.extra
            .get("chain_name")
            .and_then(|v| v.as_str())
            .map(|s| s == "test")
            .unwrap_or(false)
    }

    fn is_onion(&self) -> bool {
        self.host.ends_with(".onion")
    }

    /// Check if this server meets the minimum version requirements for the leaderboard
    fn meets_leaderboard_version_requirements(&self) -> bool {
        // Check Zebra version from zcashd_subversion (e.g., "/Zebra:3.1.0/")
        let zebra_ok = if let Some(subversion) = self.extra.get("zcashd_subversion") {
            if let Some(subversion_str) = subversion.as_str() {
                // Extract version from format like "/Zebra:3.1.0/"
                if let Some(version_part) = subversion_str
                    .strip_prefix("/Zebra:")
                    .and_then(|s| s.strip_suffix('/'))
                {
                    version_meets_minimum(version_part, LEADERBOARD_MIN_ZEBRA_VERSION)
                } else {
                    false // Not a Zebra node
                }
            } else {
                false
            }
        } else {
            false // No subversion info, can't verify Zebra
        };

        if !zebra_ok {
            return false;
        }

        // Check LWD version
        let lwd_version = self.server_version.as_deref().unwrap_or("");

        // Check if this is a Zaino server
        let is_zaino = self
            .extra
            .get("vendor")
            .and_then(|v| v.as_str())
            .map(|v| v.contains("Zaino"))
            .unwrap_or(false);

        if is_zaino {
            // For Zaino, check against LEADERBOARD_MIN_ZAINO_VERSION
            let clean_version = lwd_version.trim_start_matches('v');
            version_meets_minimum(clean_version, LEADERBOARD_MIN_ZAINO_VERSION)
        } else {
            // For regular LWD, check against LEADERBOARD_MIN_LWD_VERSION
            let clean_version = lwd_version.trim_start_matches('v');
            version_meets_minimum(clean_version, LEADERBOARD_MIN_LWD_VERSION)
        }
    }
}

/// Compare two semantic version strings, returns true if `version` >= `minimum`
fn version_meets_minimum(version: &str, minimum: &str) -> bool {
    let parse_version = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|part| {
                // Handle versions like "0.4.18-rc1" by taking only the numeric part
                part.split(|c: char| !c.is_ascii_digit())
                    .next()
                    .and_then(|n| n.parse().ok())
            })
            .collect()
    };

    let version_parts = parse_version(version);
    let minimum_parts = parse_version(minimum);

    // Compare each part
    for i in 0..minimum_parts.len().max(version_parts.len()) {
        let v = version_parts.get(i).copied().unwrap_or(0);
        let m = minimum_parts.get(i).copied().unwrap_or(0);

        if v > m {
            return true;
        }
        if v < m {
            return false;
        }
    }

    true // Equal versions
}

#[derive(Debug)]
struct SafeNetwork(&'static str);

impl SafeNetwork {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "btc" => Some(SafeNetwork("btc")),
            "zec" => Some(SafeNetwork("zec")),
            "http" => Some(SafeNetwork("http")),
            _ => None,
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
    uptime_stats: UptimeStats,
    results_window_days: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UptimeStats {
    last_day: f64,
    last_week: f64,
    last_month: f64,
    uptime_since_launch: f64,
    first_seen: String,
    total_checks: u64,
    checks_succeeded: u64,
    checks_failed: u64,
    last_check: String,
    last_online: String,
    is_currently_online: bool,
    last_day_formatted: String,
    last_week_formatted: String,
    last_month_formatted: String,
    uptime_since_launch_formatted: String,
}

#[derive(Serialize)]
struct ApiServerInfo {
    hostname: String,
    port: u16,
    protocol: &'static str,
    ping: Option<f64>,
    online: bool,
    community: bool,
    height: u64,
    uptime_30d: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_seen: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lightwallet_server_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    node_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    donation_address: Option<String>,
}

#[derive(Serialize)]
struct ApiResponse {
    servers: Vec<ApiServerInfo>,
}

#[derive(Deserialize)]
struct IndexQuery {
    hide_community: Option<bool>,
    tor_only: Option<bool>,
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
            url: format!(
                "http://{}:{}",
                env::var("CLICKHOUSE_HOST").unwrap_or_else(|_| "chronicler".into()),
                env::var("CLICKHOUSE_PORT").unwrap_or_else(|_| "8123".into())
            ),
            user: env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "hosh".into()),
            password: env::var("CLICKHOUSE_PASSWORD")
                .expect("CLICKHOUSE_PASSWORD environment variable must be set"),
            database: env::var("CLICKHOUSE_DB").unwrap_or_else(|_| "hosh".into()),
        }
    }
}

#[derive(Clone)]
struct Config {
    results_window_days: u64,
    api_key: String,
}

impl Config {
    fn from_env() -> Result<Self, actix_web::Error> {
        let results_window_days = env::var("RESULTS_WINDOW_DAYS")
            .unwrap_or_else(|_| "1".to_string())
            .parse()
            .map_err(|e| {
                warn!("Failed to parse RESULTS_WINDOW_DAYS: {}", e);
                actix_web::error::ErrorBadRequest(format!(
                    "Invalid RESULTS_WINDOW_DAYS value: {}",
                    e
                ))
            })?;

        let api_key = env::var("API_KEY").unwrap_or_else(|_| {
            warn!("API_KEY not set, using default insecure key");
            "insecure-default-key".to_string()
        });

        Ok(Self {
            results_window_days,
            api_key,
        })
    }
}

#[derive(Clone)]
struct CacheEntry {
    html: String,
    timestamp: std::time::Instant,
}

type PageCache = Arc<RwLock<HashMap<String, CacheEntry>>>;

#[derive(Clone)]
struct Worker {
    clickhouse: ClickhouseConfig,
    http_client: reqwest::Client,
    config: Config,
    cache: PageCache,
}

#[get("/")]
async fn root() -> Result<Redirect> {
    Ok(Redirect::to("/zec"))
}

/// Helper function to fetch and render the network status page
async fn fetch_and_render_network_status(
    worker: &Worker,
    network: &SafeNetwork,
    hide_community: bool,
    tor_only: bool,
) -> Result<String> {
    // Update query to handle empty results and use FORMAT JSONEachRow, including 30-day uptime and community flag
    // For ZEC, use max-check-based calculation. For other networks, use simple check-based calculation.
    // Both ZEC and BTC use the same formula:
    // uptime = (checks_succeeded / total_checks) * percentage_of_month_announced
    // where percentage_of_month_announced = min(days_since_first_seen, 30) / 30
    let query = format!(
        r#"
        WITH latest_results AS (
            SELECT
                r.*,
                ROW_NUMBER() OVER (PARTITION BY r.hostname, r.port ORDER BY r.checked_at DESC) as rn
            FROM {}.results r
            WHERE r.checker_module = '{}'
            AND r.checked_at >= now() - INTERVAL {} DAY
        ),
        -- Calculate first_seen and percentage of month for each server
        first_seen_per_server AS (
            SELECT
                hostname,
                toString(port) as port,
                min(checked_at) as first_seen,
                least(dateDiff('day', min(checked_at), now()), 30) / 30.0 as percentage_of_month
            FROM {}.results
            WHERE checker_module = '{}'
            GROUP BY hostname, port
        ),
        uptime_30_day AS (
            SELECT
                u.hostname,
                u.port,
                -- (checks_succeeded / total_checks) * percentage_of_month_announced
                (sum(u.online_count) * 100.0 / greatest(sum(u.total_checks), 1)) * fs.percentage_of_month as uptime_percentage
            FROM {}.uptime_stats_by_port u
            LEFT JOIN first_seen_per_server fs ON u.hostname = fs.hostname AND u.port = fs.port
            WHERE u.time_bucket >= now() - INTERVAL 30 DAY
            GROUP BY u.hostname, u.port, fs.percentage_of_month
        )
        SELECT
            lr.hostname,
            lr.checked_at,
            lr.status,
            lr.ping_ms as ping,
            lr.response_data,
            u30.uptime_percentage as uptime_30_day,
            t.community
        FROM latest_results lr
        LEFT JOIN uptime_30_day u30 ON lr.hostname = u30.hostname AND toString(lr.port) = u30.port
        LEFT JOIN {}.targets t ON lr.hostname = t.hostname AND lr.port = t.port AND lr.checker_module = t.module
        WHERE lr.rn = 1
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database,
        network.0,
        worker.config.results_window_days,
        worker.clickhouse.database,
        network.0,
        worker.clickhouse.database,
        worker.clickhouse.database
    );

    info!(
        "Executing ClickHouse query for network {} with window of {} days",
        network.0, worker.config.results_window_days
    );

    // Add query settings via URL parameters to limit memory usage
    let url_with_params = format!(
        "{}?max_memory_usage=4000000000&max_bytes_before_external_sort=2000000000",
        worker.clickhouse.url
    );

    let response = worker
        .http_client
        .post(&url_with_params)
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
        return Err(actix_web::error::ErrorInternalServerError(
            "Database query failed",
        ));
    }

    // Handle empty response case
    if body.trim().is_empty() {
        info!("No results found for network {}", network.0);
        let template = IndexTemplate {
            servers: Vec::new(),
            percentile_height: 0,
            current_network: network.0,
            total_count: 0,
            community_count: 0,
            hide_community,
            tor_only,
            onion_count: 0,
        };

        let html = template.render().map_err(|e| {
            error!("Template rendering error: {}", e);
            actix_web::error::ErrorInternalServerError("Template rendering failed")
        })?;

        return Ok(html);
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
                    warn!(
                        "Empty or invalid response_data for host: {}",
                        result["hostname"].as_str().unwrap_or("unknown")
                    );
                    continue;
                }

                // Try to validate and fix the JSON if needed
                let cleaned_response_data =
                    validate_and_fix_json(response_data).unwrap_or_else(|| {
                        let hostname = result["hostname"].as_str().unwrap_or("unknown");
                        warn!("Could not fix malformed JSON for host: {}", hostname);

                        // Log the problematic JSON for debugging
                        log_problematic_json(hostname, response_data);

                        // Try to get more detailed error information
                        if let Err(detailed_error) = validate_json_with_details(response_data) {
                            warn!(
                                "JSON validation details for host {}: {}",
                                hostname, detailed_error
                            );
                        }

                        "{}".to_string()
                    });

                // Try to parse the response_data as ServerInfo
                match serde_json::from_str::<ServerInfo>(&cleaned_response_data) {
                    Ok(mut server_info) => {
                        // Add the uptime_30_day from the query result
                        server_info.uptime_30_day =
                            result.get("uptime_30_day").and_then(|v| v.as_f64());
                        // Add the community flag from the query result
                        server_info.community = result
                            .get("community")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);

                        // Store the first_seen field in the extra HashMap for later use
                        if let Some(first_seen_str) =
                            result.get("first_seen").and_then(|v| v.as_str())
                        {
                            server_info.extra.insert(
                                "first_seen".to_string(),
                                serde_json::Value::String(first_seen_str.to_string()),
                            );
                        }

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
                                error_message: Some(
                                    "Server response could not be parsed".to_string(),
                                ),
                                last_updated: result["checked_at"].as_str().map(|s| s.to_string()),
                                ping: result["ping"].as_f64(),
                                server_version: None,
                                user_submitted: false,
                                community: result
                                    .get("community")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false),
                                check_id: None,
                                extra: HashMap::new(),
                                uptime_30_day: result.get("uptime_30_day").and_then(|v| v.as_f64()),
                            };

                            // Try to extract basic information from the raw response_data
                            if let Ok(raw_value) =
                                serde_json::from_str::<serde_json::Value>(&cleaned_response_data)
                            {
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
                                            fallback_server.server_version =
                                                Some(version.to_string());
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
                                            fallback_server.error_type =
                                                Some(error_type.to_string());
                                        }
                                    }

                                    if let Some(error_msg_val) = obj.get("error_message") {
                                        if let Some(error_msg) = error_msg_val.as_str() {
                                            fallback_server.error_message =
                                                Some(extract_error_info(error_msg));
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
                                        if ![
                                            "host",
                                            "port",
                                            "height",
                                            "status",
                                            "error",
                                            "error_type",
                                            "error_message",
                                            "last_updated",
                                            "ping",
                                            "server_version",
                                            "user_submitted",
                                            "check_id",
                                        ]
                                        .contains(&key.as_str())
                                        {
                                            fallback_server
                                                .extra
                                                .insert(key.clone(), value.clone());
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
                            error_message: Some(
                                "Server response contains invalid JSON".to_string(),
                            ),
                            last_updated: None,
                            ping: None,
                            server_version: None,
                            user_submitted: false,
                            community: false, // Default to false for error cases
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
                        ping_a
                            .partial_cmp(&ping_b)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then(a.host.to_lowercase().cmp(&b.host.to_lowercase()))
                    }
                    (Some(_), None) => std::cmp::Ordering::Less, // a has ping, b doesn't
                    (None, Some(_)) => std::cmp::Ordering::Greater, // b has ping, a doesn't
                    (None, None) => {
                        // Neither has ping, sort by hostname
                        a.host.to_lowercase().cmp(&b.host.to_lowercase())
                    }
                }
            }
            (true, false) => std::cmp::Ordering::Less, // a online, b offline
            (false, true) => std::cmp::Ordering::Greater, // b online, a offline
            (false, false) => {
                // Both offline, sort by hostname
                a.host.to_lowercase().cmp(&b.host.to_lowercase())
            }
        }
    });

    // Calculate percentile height
    let heights: Vec<u64> = servers
        .iter()
        .filter(|s| s.height > 0)
        .map(|s| s.height)
        .collect();
    let percentile_height = calculate_percentile(&heights, 90);

    let community_count = servers.iter().filter(|s| s.is_community()).count();
    let onion_count = servers.iter().filter(|s| s.is_onion()).count();

    // Filter servers based on hide_community and tor_only flags
    let filtered_servers = servers
        .into_iter()
        .filter(|s| {
            let passes_community_filter = !hide_community || !s.is_community();
            let passes_tor_filter = !tor_only || s.is_onion();
            passes_community_filter && passes_tor_filter
        })
        .collect::<Vec<_>>();

    let total_count = filtered_servers.len();

    let template = IndexTemplate {
        servers: filtered_servers,
        percentile_height,
        current_network: network.0,
        total_count,
        community_count,
        hide_community,
        tor_only,
        onion_count,
    };

    let html = template.render().map_err(|e| {
        error!("Template rendering error: {}", e);
        actix_web::error::ErrorInternalServerError("Template rendering failed")
    })?;

    Ok(html)
}

/// Helper function to fetch and render the leaderboard page (top 50 by uptime)
async fn fetch_and_render_leaderboard(
    worker: &Worker,
    network: &SafeNetwork,
) -> Result<String> {
    // Query for leaderboard - top 50 servers by 30-day uptime
    let query = format!(
        r#"
        WITH latest_results AS (
            SELECT
                r.*,
                ROW_NUMBER() OVER (PARTITION BY r.hostname, r.port ORDER BY r.checked_at DESC) as rn
            FROM {}.results r
            WHERE r.checker_module = '{}'
            AND r.checked_at >= now() - INTERVAL {} DAY
        ),
        first_seen_per_server AS (
            SELECT
                hostname,
                toString(port) as port,
                min(checked_at) as first_seen,
                least(dateDiff('day', min(checked_at), now()), 30) / 30.0 as percentage_of_month
            FROM {}.results
            WHERE checker_module = '{}'
            GROUP BY hostname, port
        ),
        uptime_30_day AS (
            SELECT
                u.hostname,
                u.port,
                (sum(u.online_count) * 100.0 / greatest(sum(u.total_checks), 1)) * fs.percentage_of_month as uptime_percentage
            FROM {}.uptime_stats_by_port u
            LEFT JOIN first_seen_per_server fs ON u.hostname = fs.hostname AND u.port = fs.port
            WHERE u.time_bucket >= now() - INTERVAL 30 DAY
            GROUP BY u.hostname, u.port, fs.percentage_of_month
        )
        SELECT
            lr.hostname,
            lr.checked_at,
            lr.status,
            lr.ping_ms as ping,
            lr.response_data,
            u30.uptime_percentage as uptime_30_day,
            t.community
        FROM latest_results lr
        LEFT JOIN uptime_30_day u30 ON lr.hostname = u30.hostname AND toString(lr.port) = u30.port
        LEFT JOIN {}.targets t ON lr.hostname = t.hostname AND lr.port = t.port AND lr.checker_module = t.module
        WHERE lr.rn = 1
        AND u30.uptime_percentage IS NOT NULL
        AND u30.uptime_percentage > 0
        ORDER BY u30.uptime_percentage DESC
        LIMIT 50
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database,
        network.0,
        worker.config.results_window_days,
        worker.clickhouse.database,
        network.0,
        worker.clickhouse.database,
        worker.clickhouse.database
    );

    info!("Executing ClickHouse leaderboard query for network {}", network.0);

    let url_with_params = format!(
        "{}?max_memory_usage=4000000000&max_bytes_before_external_sort=2000000000",
        worker.clickhouse.url
    );

    let response = worker
        .http_client
        .post(&url_with_params)
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
        return Err(actix_web::error::ErrorInternalServerError(
            "Database query failed",
        ));
    }

    // Parse results and build leaderboard entries
    let mut entries = Vec::new();
    let mut rank = 1;

    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(result) = serde_json::from_str::<serde_json::Value>(line) {
            let response_data = match result.get("response_data") {
                Some(Value::String(s)) => s.as_str(),
                _ => "{}",
            };

            let cleaned_response_data = validate_and_fix_json(response_data)
                .unwrap_or_else(|| "{}".to_string());

            if let Ok(mut server_info) = serde_json::from_str::<ServerInfo>(&cleaned_response_data) {
                server_info.uptime_30_day = result.get("uptime_30_day").and_then(|v| v.as_f64());
                server_info.community = result
                    .get("community")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                // Only include servers that meet the minimum version requirements
                if !server_info.meets_leaderboard_version_requirements() {
                    continue;
                }

                entries.push(LeaderboardEntry {
                    rank,
                    server: server_info,
                });
                rank += 1;
            }
        }
    }

    // Get percentile height for consistency with other pages
    let percentile_height = entries
        .iter()
        .map(|e| e.server.height)
        .filter(|&h| h > 0)
        .max()
        .unwrap_or(0);

    let template = LeaderboardTemplate {
        entries,
        current_network: network.0,
        percentile_height,
    };

    let html = template.render().map_err(|e| {
        error!("Template rendering error: {}", e);
        actix_web::error::ErrorInternalServerError("Template rendering failed")
    })?;

    Ok(html)
}

#[get("/{network}")]
async fn network_status(
    worker: web::Data<Worker>,
    network: web::Path<String>,
    query_params: web::Query<IndexQuery>,
) -> Result<HttpResponse> {
    let network = SafeNetwork::from_str(&network)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid network"))?;

    let hide_community = query_params.hide_community.unwrap_or(false);
    let tor_only = query_params.tor_only.unwrap_or(false);

    // ONLY serve from cache - never trigger ClickHouse queries from user requests
    // This prevents traffic spikes from overwhelming ClickHouse
    let cache_key = format!("{}-{}-{}", network.0, hide_community, tor_only);

    let cache = worker.cache.read().await;
    if let Some(entry) = cache.get(&cache_key) {
        // Serve cache regardless of age - background task keeps it fresh
        // Add X-Cache-Age header for debugging
        let cache_age_secs = entry.timestamp.elapsed().as_secs();
        info!(
            "Serving {} from cache (age: {}s)",
            cache_key, cache_age_secs
        );

        return Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .insert_header(("X-Cache-Age", cache_age_secs.to_string()))
            .insert_header(("Cache-Control", "public, max-age=10, s-maxage=10"))
            .body(entry.html.clone()));
    }

    // Cache miss - this should only happen on first startup
    // Don't trigger a query, return a friendly error and let background task populate it
    warn!(
        "Cache miss for {} - waiting for background refresh",
        cache_key
    );
    Ok(HttpResponse::ServiceUnavailable()
        .content_type("text/html; charset=utf-8")
        .body(format!(
            r#"<!DOCTYPE html>
            <html>
            <head>
                <title>Loading...</title>
                <meta http-equiv="refresh" content="2">
                <style>
                    body {{ font-family: sans-serif; text-align: center; padding: 50px; }}
                    .loading {{ font-size: 24px; color: #666; }}
                </style>
            </head>
            <body>
                <div class="loading">
                    <p>⏳ Loading network status...</p>
                    <p style="font-size: 14px; color: #999;">Cache is warming up. This page will refresh automatically.</p>
                </div>
            </body>
            </html>"#
        )))
}

#[get("/{network}/leaderboard")]
async fn leaderboard(
    worker: web::Data<Worker>,
    network: web::Path<String>,
) -> Result<HttpResponse> {
    let network = SafeNetwork::from_str(&network)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid network"))?;

    // Serve from cache
    let cache_key = format!("{}-leaderboard", network.0);

    let cache = worker.cache.read().await;
    if let Some(entry) = cache.get(&cache_key) {
        let cache_age_secs = entry.timestamp.elapsed().as_secs();
        info!(
            "Serving {} from cache (age: {}s)",
            cache_key, cache_age_secs
        );

        return Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .insert_header(("X-Cache-Age", cache_age_secs.to_string()))
            .insert_header(("Cache-Control", "public, max-age=10, s-maxage=10"))
            .body(entry.html.clone()));
    }

    // Cache miss - waiting for background refresh
    warn!(
        "Cache miss for {} - waiting for background refresh",
        cache_key
    );
    Ok(HttpResponse::ServiceUnavailable()
        .content_type("text/html; charset=utf-8")
        .body(format!(
            r#"<!DOCTYPE html>
            <html>
            <head>
                <title>Loading...</title>
                <meta http-equiv="refresh" content="2">
                <style>
                    body {{ font-family: sans-serif; text-align: center; padding: 50px; }}
                    .loading {{ font-size: 24px; color: #666; }}
                </style>
            </head>
            <body>
                <div class="loading">
                    <p>Loading leaderboard...</p>
                    <p style="font-size: 14px; color: #999;">Cache is warming up. This page will refresh automatically.</p>
                </div>
            </body>
            </html>"#
        )))
}

#[get("/{network}/{host}")]
async fn server_detail(
    worker: web::Data<Worker>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let (network, host_with_port) = path.into_inner();

    // Parse hostname:port format
    let (host, port) = if let Some(colon_pos) = host_with_port.rfind(':') {
        let hostname = &host_with_port[..colon_pos];
        let port_str = &host_with_port[colon_pos + 1..];
        if let Ok(port_num) = port_str.parse::<u16>() {
            (hostname.to_string(), Some(port_num))
        } else {
            (host_with_port.clone(), None)
        }
    } else {
        (host_with_port.clone(), None)
    };
    let safe_network = SafeNetwork::from_str(&network)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid network"))?;

    // Query the targets table to get server information
    let query = format!(
        r#"
        WITH latest_results AS (
            SELECT
                r.*,
                ROW_NUMBER() OVER (PARTITION BY r.hostname, r.port ORDER BY r.checked_at DESC) as rn
            FROM {}.results r
            WHERE r.checker_module = '{}'
            AND r.hostname = '{}'
            AND r.checked_at >= now() - INTERVAL {} DAY
            {}
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
        worker.config.results_window_days,
        if let Some(port_num) = port {
            format!("AND r.port = {}", port_num)
        } else {
            String::new()
        }
    );

    let response = worker
        .http_client
        .post(&worker.clickhouse.url)
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
        return Err(actix_web::error::ErrorInternalServerError(
            "Database query failed",
        ));
    }

    // Parse the response data
    let mut data: HashMap<String, Value> = HashMap::new();
    if !body.trim().is_empty() {
        if let Ok(result) = serde_json::from_str::<serde_json::Value>(body.lines().next().unwrap())
        {
            if let Some(response_data) = result["response_data"].as_str() {
                if let Ok(parsed_data) =
                    serde_json::from_str::<HashMap<String, Value>>(response_data)
                {
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
                ROW_NUMBER() OVER (PARTITION BY r.hostname, r.port ORDER BY r.checked_at DESC) as rn
            FROM {}.results r
            WHERE r.checker_module = '{}'
            AND r.hostname = '{}'
            AND r.checked_at >= now() - INTERVAL 1 DAY
            {}
        )
        SELECT
            hostname,
            response_data
        FROM latest_results
        WHERE rn = 1
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database,
        safe_network.0,
        host,
        if let Some(port_num) = port {
            format!("AND r.port = {}", port_num)
        } else {
            String::new()
        }
    );

    let count_response = worker
        .http_client
        .post(&worker.clickhouse.url)
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
    }

    let percentile_height = calculate_percentile(&heights, 90);

    // Calculate uptime statistics
    let uptime_stats = calculate_uptime_stats(&worker, &host, &network, port).await?;

    // Create sorted data for alphabetical display
    let mut sorted_data: Vec<(String, Value)> =
        data.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    sorted_data.sort_by(|a, b| a.0.cmp(&b.0));

    // Extract donation_address if it exists
    let donation_opt = data.get("donation_address").and_then(|v| v.as_str());
    let donation_address = donation_opt.unwrap_or("").to_string();
    let show_donation = !donation_address.trim().is_empty();

    // Generate QR code SVG for donation address
    let donation_qr_code = if show_donation {
        match QrCode::new(&donation_address) {
            Ok(code) => code
                .render()
                .min_dimensions(200, 200)
                .dark_color(svg::Color("#000000"))
                .light_color(svg::Color("#FFFFFF"))
                .build(),
            Err(_) => String::new(),
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
        uptime_stats,
        results_window_days: worker.config.results_window_days,
    };

    let html = template.render().map_err(|e| {
        error!("Template rendering error: {}", e);
        actix_web::error::ErrorInternalServerError("Template rendering failed")
    })?;

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .insert_header(("Cache-Control", "public, max-age=10, s-maxage=10"))
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
    // uptime = (checks_succeeded / total_checks) * percentage_of_month_announced
    // where percentage_of_month_announced = min(days_since_first_seen, 30) / 30
    let query = format!(
        r#"
        WITH latest_results AS (
            SELECT
                r.*,
                ROW_NUMBER() OVER (PARTITION BY r.hostname, r.port ORDER BY r.checked_at DESC) as rn
            FROM {}.results r
            WHERE r.checker_module = '{}'
            AND r.checked_at >= now() - INTERVAL {} DAY
        ),
        -- Calculate first_seen and percentage of month for each server
        first_seen_per_server AS (
            SELECT
                hostname,
                toString(port) as port,
                min(checked_at) as first_seen,
                least(dateDiff('day', min(checked_at), now()), 30) / 30.0 as percentage_of_month
            FROM {}.results
            WHERE checker_module = '{}'
            GROUP BY hostname, port
        ),
        uptime_30_day AS (
            SELECT
                u.hostname,
                u.port,
                -- (checks_succeeded / total_checks) * percentage_of_month_announced
                (sum(u.online_count) * 100.0 / greatest(sum(u.total_checks), 1)) * fs.percentage_of_month as uptime_percentage
            FROM {}.uptime_stats_by_port u
            LEFT JOIN first_seen_per_server fs ON u.hostname = fs.hostname AND u.port = fs.port
            WHERE u.time_bucket >= now() - INTERVAL 30 DAY
            GROUP BY u.hostname, u.port, fs.percentage_of_month
        )
        SELECT
            lr.hostname,
            lr.checked_at,
            lr.status,
            lr.ping_ms as ping,
            lr.response_data,
            u30.uptime_percentage as uptime_30_day,
            t.community
        FROM latest_results lr
        LEFT JOIN uptime_30_day u30 ON lr.hostname = u30.hostname AND toString(lr.port) = u30.port
        LEFT JOIN {}.targets t ON lr.hostname = t.hostname AND lr.port = t.port AND lr.checker_module = t.module
        WHERE lr.rn = 1
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database,
        network.0,
        worker.config.results_window_days,
        worker.clickhouse.database,
        network.0,
        worker.clickhouse.database,
        worker.clickhouse.database
    );

    let response = worker
        .http_client
        .post(&worker.clickhouse.url)
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
        return Err(actix_web::error::ErrorInternalServerError(
            "Database query failed",
        ));
    }

    let mut servers = Vec::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(result) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(response_data) = result["response_data"].as_str() {
                if let Ok(mut server_info) = serde_json::from_str::<ServerInfo>(response_data) {
                    // Add the uptime_30_day from the query result
                    server_info.uptime_30_day =
                        result.get("uptime_30_day").and_then(|v| v.as_f64());
                    // Add the community flag from the query result
                    server_info.community = result
                        .get("community")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    // Store the first_seen field in the extra HashMap for later use
                    if let Some(first_seen_str) = result.get("first_seen").and_then(|v| v.as_str())
                    {
                        server_info.extra.insert(
                            "first_seen".to_string(),
                            serde_json::Value::String(first_seen_str.to_string()),
                        );
                    }

                    servers.push(server_info);
                }
            }
        }
    }

    let api_servers: Vec<ApiServerInfo> = servers
        .into_iter()
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
                community: server.community,
                height: server.height,
                uptime_30d: server.uptime_30_day.map(|p| p / 100.0),
                first_seen: server
                    .extra
                    .get("first_seen")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                lightwallet_server_version: server.server_version.clone(),
                node_version: match network.0 {
                    "zec" => server
                        .extra
                        .get("zcashd_subversion")
                        .and_then(|v| v.as_str())
                        .map(|s| s.replace('/', "")),
                    "btc" => server.server_version.clone(),
                    _ => None,
                },
                donation_address: server
                    .extra
                    .get("donation_address")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.to_string()),
            }
        })
        .collect();

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .insert_header(("Cache-Control", "public, max-age=10, s-maxage=10"))
        .json(ApiResponse {
            servers: api_servers,
        }))
}

// Struct for job requests
#[derive(Debug, Deserialize, Serialize)]
struct CheckRequest {
    host: String,
    port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    check_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_submitted: Option<bool>,
}

// Struct for check results
#[derive(Debug, Deserialize, Serialize)]
struct CheckResult {
    hostname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    checker_module: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ping_ms: Option<f64>,
    response_data: String,
}

// GET /api/v1/jobs - Returns servers that need to be checked
#[get("/api/v1/jobs")]
async fn get_jobs(
    worker: web::Data<Worker>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    // Verify API key
    let api_key = query
        .get("api_key")
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Missing API key"))?;

    if api_key != &worker.config.api_key {
        return Err(actix_web::error::ErrorUnauthorized("Invalid API key"));
    }

    let checker_module = query
        .get("checker_module")
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Missing checker_module parameter"))?;

    let limit: u32 = query
        .get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(10);

    info!(
        "📡 get_jobs request: checker_module={}, limit={}",
        checker_module, limit
    );

    // Fetch all targets for this module
    let targets_query = format!(
        r#"
        SELECT hostname as host, port
        FROM {}.targets
        WHERE module = '{}'
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database, checker_module
    );

    let targets_response = worker
        .http_client
        .post(&worker.clickhouse.url)
        .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
        .header("Content-Type", "text/plain")
        .body(targets_query)
        .send()
        .await
        .map_err(|e| {
            error!("ClickHouse targets query error: {}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    if !targets_response.status().is_success() {
        let err_body = targets_response.text().await.unwrap_or_default();
        error!("ClickHouse targets query failed: {}", err_body);
        return Err(actix_web::error::ErrorInternalServerError(
            "Database query failed",
        ));
    }

    let targets_body = targets_response.text().await.map_err(|e| {
        error!("Failed to read targets response: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to read database response")
    })?;

    info!(
        "📦 Raw targets response ({} bytes): {}",
        targets_body.len(),
        if targets_body.len() < 200 {
            &targets_body
        } else {
            &targets_body[..200]
        }
    );

    // Parse all targets
    let mut all_targets = Vec::new();
    for line in targets_body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(mut job) = serde_json::from_str::<CheckRequest>(line) {
            // Normalize port: if it's 0 or missing, use default 50002
            if job.port == 0 {
                job.port = 50002;
            }
            all_targets.push((job.host.clone(), job.port));
        }
    }

    info!(
        "📋 Found {} total targets for module={}",
        all_targets.len(),
        checker_module
    );

    // Fetch recently checked (hostname, port) pairs
    let recent_checks_query = format!(
        r#"
        SELECT DISTINCT
            hostname as host,
            port
        FROM {}.results
        WHERE checker_module = '{}'
        AND checked_at >= now() - INTERVAL 5 MINUTE
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database, checker_module
    );

    let recent_response = worker
        .http_client
        .post(&worker.clickhouse.url)
        .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
        .header("Content-Type", "text/plain")
        .body(recent_checks_query)
        .send()
        .await
        .map_err(|e| {
            error!("ClickHouse recent checks query error: {}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    if !recent_response.status().is_success() {
        let err_body = recent_response.text().await.unwrap_or_default();
        error!("ClickHouse recent checks query failed: {}", err_body);
        return Err(actix_web::error::ErrorInternalServerError(
            "Database query failed",
        ));
    }

    let recent_body = recent_response.text().await.map_err(|e| {
        error!("Failed to read recent checks response: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to read database response")
    })?;

    // Parse recently checked servers into a HashSet for fast lookup
    let mut recently_checked = std::collections::HashSet::new();
    for line in recent_body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(job) = serde_json::from_str::<CheckRequest>(line) {
            let port = if job.port == 0 { 50002 } else { job.port };
            recently_checked.insert((job.host, port));
        }
    }

    info!(
        "🔍 Found {} recently checked servers (last 5 min) for module={}",
        recently_checked.len(),
        checker_module
    );

    // Filter targets to exclude recently checked ones
    let mut jobs = Vec::new();
    for (host, port) in all_targets {
        if !recently_checked.contains(&(host.clone(), port)) {
            jobs.push(CheckRequest {
                host,
                port,
                check_id: None,
                user_submitted: None,
            });

            if jobs.len() >= limit as usize {
                break;
            }
        }
    }

    info!(
        "📤 Returning {} jobs for checker_module={}",
        jobs.len(),
        checker_module
    );

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .json(jobs))
}

// POST /api/v1/results - Accepts check results
#[post("/api/v1/results")]
async fn post_results(
    worker: web::Data<Worker>,
    query: web::Query<HashMap<String, String>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse> {
    // Verify API key
    let api_key = query
        .get("api_key")
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Missing API key"))?;

    if api_key != &worker.config.api_key {
        return Err(actix_web::error::ErrorUnauthorized("Invalid API key"));
    }

    info!("📥 Received check result");

    // Extract fields from the result
    let hostname = body
        .get("hostname")
        .or_else(|| body.get("host"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Missing hostname/host field"))?;

    let checker_module = body
        .get("checker_module")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let status = body
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let port = body.get("port").and_then(|v| v.as_u64()).unwrap_or(50002) as u16;

    let ping_ms = body
        .get("ping_ms")
        .or_else(|| body.get("ping"))
        .and_then(|v| v.as_f64());

    // Extract fields we want to persist forever (before response_data TTL clears them)
    let server_version = body
        .get("server_version")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let error = body
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let block_height = body
        .get("height")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // Serialize the full response data as JSON (will be TTL'd after 7 days)
    let response_data = serde_json::to_string(&body.0).unwrap_or_default();

    // Insert into ClickHouse with extracted columns that persist forever
    let insert_query = format!(
        "INSERT INTO {}.results (hostname, checker_module, status, ping_ms, port, server_version, error, block_height, response_data, checked_at) FORMAT JSONEachRow",
        worker.clickhouse.database
    );

    let result_json = serde_json::json!({
        "hostname": hostname,
        "checker_module": checker_module,
        "status": status,
        "ping_ms": ping_ms,
        "port": port,
        "server_version": server_version,
        "error": error,
        "block_height": block_height,
        "response_data": response_data,
        "checked_at": chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
    });

    let response = worker
        .http_client
        .post(&worker.clickhouse.url)
        .basic_auth(&worker.clickhouse.user, Some(&worker.clickhouse.password))
        .header("Content-Type", "application/json")
        .body(result_json.to_string())
        .query(&[("query", insert_query)])
        .send()
        .await
        .map_err(|e| {
            error!("ClickHouse insert error: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to insert result")
        })?;

    if !response.status().is_success() {
        let error_body = response.text().await.unwrap_or_default();
        error!("ClickHouse insert failed: {}", error_body);
        return Err(actix_web::error::ErrorInternalServerError(
            "Failed to insert result",
        ));
    }

    info!("✅ Successfully stored result for {}:{}", hostname, port);

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Result stored successfully"
    })))
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

async fn calculate_uptime_stats(
    worker: &Worker,
    host: &str,
    _network: &str,
    port: Option<u16>,
) -> Result<UptimeStats, actix_web::Error> {
    // Query for uptime statistics using the port-aware uptime_stats_by_port materialized view
    // port_filter is for uptime_stats_by_port (port is String)
    // port_filter_results is for results table (port is UInt16)
    let (port_filter, port_filter_results) = if let Some(port_num) = port {
        (format!("AND port = '{}'", port_num), format!("AND port = {}", port_num))
    } else {
        (String::new(), String::new())
    };

    let uptime_query = format!(
        r#"
        WITH first_seen_date AS (
            SELECT min(checked_at) as first_seen
            FROM {}.results
            WHERE hostname = '{}'
            {}
        ),
        -- Calculate the percentage of the 30-day period that the server has been announced
        -- If first_seen is within the last 30 days, this will be < 1.0
        -- If first_seen is 30+ days ago, this will be 1.0
        days_announced AS (
            SELECT
                least(dateDiff('day', first_seen, now()), 30) as days_in_period,
                least(dateDiff('day', first_seen, now()), 30) / 30.0 as percentage_of_month
            FROM first_seen_date
        )
        SELECT
            'day' as period,
            sum(online_count) * 100.0 / greatest(sum(total_checks), 1) as uptime_percentage
        FROM {}.uptime_stats_by_port
        WHERE hostname = '{}'
        AND time_bucket >= now() - INTERVAL 1 DAY
        {}

        UNION ALL

        SELECT
            'week' as period,
            sum(online_count) * 100.0 / greatest(sum(total_checks), 1) as uptime_percentage
        FROM {}.uptime_stats_by_port
        WHERE hostname = '{}'
        AND time_bucket >= now() - INTERVAL 7 DAY
        {}

        UNION ALL

        -- 30-day uptime: (checks_succeeded / total_checks) * percentage_of_month_announced
        -- This penalizes newly announced servers proportionally to how long they've been known
        SELECT
            'month' as period,
            (sum(online_count) * 100.0 / greatest(sum(total_checks), 1)) * (SELECT percentage_of_month FROM days_announced) as uptime_percentage
        FROM {}.uptime_stats_by_port
        WHERE hostname = '{}'
        AND time_bucket >= now() - INTERVAL 30 DAY
        {}

        UNION ALL

        SELECT
            'since_launch' as period,
            sum(u.online_count) * 100.0 / greatest(sum(u.total_checks), 1) as uptime_percentage
        FROM {}.uptime_stats_by_port u
        CROSS JOIN first_seen_date fs
        WHERE u.hostname = '{}'
        AND u.time_bucket >= fs.first_seen
        {}
        GROUP BY fs.first_seen

        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database, // first_seen_date CTE - {}.results
        host,                       // first_seen_date CTE - hostname = '{}'
        port_filter_results,        // first_seen_date CTE - port filter (results table uses UInt16)
        worker.clickhouse.database, // day query - {}.uptime_stats_by_port
        host,                       // day query - hostname = '{}'
        port_filter,                // day query - port filter
        worker.clickhouse.database, // week query - {}.uptime_stats_by_port
        host,                       // week query - hostname = '{}'
        port_filter,                // week query - port filter
        worker.clickhouse.database, // month query - {}.uptime_stats_by_port
        host,                       // month query - hostname = '{}'
        port_filter,                // month query - port filter
        worker.clickhouse.database, // since_launch query - {}.uptime_stats_by_port
        host,                       // since_launch query - u.hostname = '{}'
        port_filter                 // since_launch query - port filter
    );

    let response = worker
        .http_client
        .post(&worker.clickhouse.url)
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
        error!(
            "ClickHouse uptime query failed with status {}: {}",
            status, body
        );
        return Err(actix_web::error::ErrorInternalServerError(
            "Database query failed",
        ));
    }

    // Parse the uptime statistics
    let mut last_day = 0.0;
    let mut last_week = 0.0;
    let mut last_month = 0.0;
    let mut uptime_since_launch = 0.0;

    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(result) = serde_json::from_str::<serde_json::Value>(line) {
            if let (Some(period), Some(uptime)) = (
                result["period"].as_str(),
                result["uptime_percentage"].as_f64(),
            ) {
                match period {
                    "day" => last_day = uptime,
                    "week" => last_week = uptime,
                    "month" => last_month = uptime,
                    "since_launch" => uptime_since_launch = uptime,
                    _ => {}
                }
            }
        }
    }

    // Get total checks, last check time, last online time, first_seen, and current status
    let port_filter = if let Some(port_num) = port {
        format!("AND port = {}", port_num)
    } else {
        String::new()
    };

    let stats_query = format!(
        r#"
        WITH latest_check AS (
            SELECT status, checked_at
            FROM {}.results
            WHERE hostname = '{}'
            {}
            ORDER BY checked_at DESC
            LIMIT 1
        ),
        first_seen_ever AS (
            SELECT min(checked_at) as first_seen
            FROM {}.results
            WHERE hostname = '{}'
            {}
        )
        SELECT
            count(*) as total_checks,
            countIf(status = 'online') as checks_succeeded,
            countIf(status != 'online') as checks_failed,
            max(checked_at) as last_check,
            max(CASE WHEN status = 'online' THEN checked_at END) as last_online,
            (SELECT first_seen FROM first_seen_ever) as first_seen,
            (SELECT status FROM latest_check) as current_status
        FROM {}.results
        WHERE hostname = '{}'
        AND checked_at >= now() - INTERVAL 30 DAY
        {}
        FORMAT JSONEachRow
        "#,
        worker.clickhouse.database,
        host,
        port_filter,
        worker.clickhouse.database,
        host,
        port_filter,
        worker.clickhouse.database,
        host,
        port_filter
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

    let debug_response = worker
        .http_client
        .post(&worker.clickhouse.url)
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

    let stats_response = worker
        .http_client
        .post(&worker.clickhouse.url)
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
    let mut checks_succeeded = 0u64;
    let mut checks_failed = 0u64;
    let mut last_check = String::new();
    let mut last_online = String::new();
    let mut first_seen = String::new();
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

            // Handle checks_succeeded
            if let Some(succeeded) = result["checks_succeeded"].as_u64() {
                checks_succeeded = succeeded;
            } else if let Some(succeeded_str) = result["checks_succeeded"].as_str() {
                if let Ok(succeeded) = succeeded_str.parse::<u64>() {
                    checks_succeeded = succeeded;
                }
            }

            // Handle checks_failed
            if let Some(failed) = result["checks_failed"].as_u64() {
                checks_failed = failed;
            } else if let Some(failed_str) = result["checks_failed"].as_str() {
                if let Ok(failed) = failed_str.parse::<u64>() {
                    checks_failed = failed;
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

            // Handle first_seen
            if let Some(seen_time) = result["first_seen"].as_str() {
                first_seen = seen_time.to_string();
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

    // Format timestamps for better display (remove milliseconds and add relative time)
    let format_timestamp = |timestamp: &str| -> (String, String) {
        if timestamp.is_empty() {
            return (String::new(), String::new());
        }

        // Parse the timestamp
        let parsed_time = DateTime::parse_from_rfc3339(timestamp)
            .ok()
            .or_else(|| parse_rfc3339_with_nanos(timestamp));

        if let Some(time) = parsed_time {
            // Format without milliseconds
            let formatted = time.format("%Y-%m-%d %H:%M:%S").to_string();

            // Calculate relative time
            let now = Utc::now().with_timezone(time.offset());
            let duration = now.signed_duration_since(time);
            let total_seconds = duration.num_seconds();

            let relative = if total_seconds < 0 {
                "just now".to_string()
            } else if total_seconds < 60 {
                format!("{}s ago", total_seconds)
            } else if total_seconds < 3600 {
                let minutes = total_seconds / 60;
                let seconds = total_seconds % 60;
                format!("{}m {}s ago", minutes, seconds)
            } else if total_seconds < 86400 {
                let hours = total_seconds / 3600;
                let mins = (total_seconds % 3600) / 60;
                format!("{}h {}m ago", hours, mins)
            } else {
                let days = total_seconds / 86400;
                let hrs = (total_seconds % 86400) / 3600;
                format!("{}d {}h ago", days, hrs)
            };

            (formatted, relative)
        } else {
            (timestamp.to_string(), String::new())
        }
    };

    let (last_check_formatted, last_check_relative) = format_timestamp(&last_check);
    let (last_online_formatted, last_online_relative) = format_timestamp(&last_online);
    let (first_seen_formatted, first_seen_relative) = format_timestamp(&first_seen);

    // Combine formatted timestamp with relative time
    let last_check_display = if !last_check_relative.is_empty() {
        format!("{} ({})", last_check_formatted, last_check_relative)
    } else {
        last_check_formatted
    };

    let last_online_display = if !last_online_relative.is_empty() {
        format!("{} ({})", last_online_formatted, last_online_relative)
    } else {
        last_online_formatted
    };

    let first_seen_display = if !first_seen_relative.is_empty() {
        format!("{} ({})", first_seen_formatted, first_seen_relative)
    } else if !first_seen.is_empty() {
        first_seen_formatted
    } else {
        String::new()
    };

    Ok(UptimeStats {
        last_day,
        last_week,
        last_month,
        uptime_since_launch,
        first_seen: first_seen_display,
        total_checks,
        checks_succeeded,
        checks_failed,
        last_check: last_check_display,
        last_online: last_online_display,
        is_currently_online,
        last_day_formatted: format!("{:.5}%", last_day),
        last_week_formatted: format!("{:.5}%", last_week),
        last_month_formatted: format!("{:.5}%", last_month),
        uptime_since_launch_formatted: format!("{:.5}%", uptime_since_launch),
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
        let height = deserialize_height(serde::Deserializer::from(
            serde_json::to_value(result["height"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(height, 12345);

        // Test string height
        let json = r#"{"height":"0"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let height = deserialize_height(serde::Deserializer::from(
            serde_json::to_value(result["height"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(height, 0);

        // Test empty string height
        let json = r#"{"height":""}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let height = deserialize_height(serde::Deserializer::from(
            serde_json::to_value(result["height"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(height, 0);

        // Test null height
        let json = r#"{"height":null}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let height = deserialize_height(serde::Deserializer::from(
            serde_json::to_value(result["height"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(height, 0);
    }

    #[test]
    fn test_deserialize_ping() {
        // Test number ping
        let json = r#"{"ping":123.45}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let ping = deserialize_ping(serde::Deserializer::from(
            serde_json::to_value(result["ping"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(ping, Some(123.45));

        // Test string ping
        let json = r#"{"ping":"123.45"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let ping = deserialize_ping(serde::Deserializer::from(
            serde_json::to_value(result["ping"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(ping, Some(123.45));

        // Test empty string ping
        let json = r#"{"ping":""}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let ping = deserialize_ping(serde::Deserializer::from(
            serde_json::to_value(result["ping"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(ping, None);

        // Test null ping
        let json = r#"{"ping":null}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let ping = deserialize_ping(serde::Deserializer::from(
            serde_json::to_value(result["ping"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(ping, None);
    }

    #[test]
    fn test_deserialize_user_submitted() {
        // Test boolean true
        let json = r#"{"user_submitted":true}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(
            serde_json::to_value(result["user_submitted"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(user_submitted, true);

        // Test boolean false
        let json = r#"{"user_submitted":false}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(
            serde_json::to_value(result["user_submitted"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(user_submitted, false);

        // Test string "true"
        let json = r#"{"user_submitted":"true"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(
            serde_json::to_value(result["user_submitted"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(user_submitted, true);

        // Test string "false"
        let json = r#"{"user_submitted":"false"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(
            serde_json::to_value(result["user_submitted"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(user_submitted, false);

        // Test string "FALSE" (case insensitive)
        let json = r#"{"user_submitted":"FALSE"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(
            serde_json::to_value(result["user_submitted"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(user_submitted, false);

        // Test number 1 (true)
        let json = r#"{"user_submitted":1}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(
            serde_json::to_value(result["user_submitted"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(user_submitted, true);

        // Test number 0 (false)
        let json = r#"{"user_submitted":0}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(
            serde_json::to_value(result["user_submitted"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(user_submitted, false);

        // Test null (defaults to false)
        let json = r#"{"user_submitted":null}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let user_submitted = deserialize_user_submitted(serde::Deserializer::from(
            serde_json::to_value(result["user_submitted"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(user_submitted, false);
    }

    #[test]
    fn test_deserialize_error_field() {
        // Test boolean true
        let json = r#"{"error":true}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let error = deserialize_error_field(serde::Deserializer::from(
            serde_json::to_value(result["error"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(error, Some("Server error occurred".to_string()));

        // Test boolean false
        let json = r#"{"error":false}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let error = deserialize_error_field(serde::Deserializer::from(
            serde_json::to_value(result["error"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(error, None);

        // Test string error
        let json = r#"{"error":"Connection failed"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let error = deserialize_error_field(serde::Deserializer::from(
            serde_json::to_value(result["error"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(error, Some("Connection failed"));

        // Test null
        let json = r#"{"error":null}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let error = deserialize_error_field(serde::Deserializer::from(
            serde_json::to_value(result["error"]).unwrap(),
        ))
        .unwrap();
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
        let host = deserialize_host(serde::Deserializer::from(
            serde_json::to_value(result["host"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(host, "128.0.190.26");

        // Test unquoted hostname
        let json = r#"{"host":"example.com"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let host = deserialize_host(serde::Deserializer::from(
            serde_json::to_value(result["host"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(host, "example.com");
    }

    #[test]
    fn test_deserialize_server_version() {
        // Test quoted server version
        let json = r#"{"server_version":"'unknown'"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let version = deserialize_server_version(serde::Deserializer::from(
            serde_json::to_value(result["server_version"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(version, Some("unknown".to_string()));

        // Test unquoted server version
        let json = r#"{"server_version":"ElectrumX 1.16.0"}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let version = deserialize_server_version(serde::Deserializer::from(
            serde_json::to_value(result["server_version"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(version, Some("ElectrumX 1.16.0".to_string()));

        // Test null server version
        let json = r#"{"server_version":null}"#;
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        let version = deserialize_server_version(serde::Deserializer::from(
            serde_json::to_value(result["server_version"]).unwrap(),
        ))
        .unwrap();
        assert_eq!(version, None);
    }
}

/// Background task to refresh the cache periodically
async fn cache_refresh_task(worker: Worker) {
    // Increase interval to reduce load - env var or default to 20 seconds
    let refresh_interval_secs = env::var("CACHE_REFRESH_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(20);

    // Refresh cache for each network, hide_community, and tor_only combination
    let networks = vec!["zec", "btc"];
    let hide_community_options = vec![false, true];
    let tor_only_options = vec![false, true];

    // Populate cache immediately on startup (before starting the interval loop)
    info!("Initial cache population on startup");
    let cycle_start = std::time::Instant::now();

    for network_str in &networks {
        for &hide_community in &hide_community_options {
            for &tor_only in &tor_only_options {
                let cache_key = format!("{}-{}-{}", network_str, hide_community, tor_only);

                if let Some(network) = SafeNetwork::from_str(network_str) {
                    let query_start = std::time::Instant::now();

                    let result = fetch_and_render_network_status(
                        &worker,
                        &network,
                        hide_community,
                        tor_only,
                    )
                    .await;
                    match result {
                        Ok(html) => {
                            let mut cache = worker.cache.write().await;
                            cache.insert(
                                cache_key.clone(),
                                CacheEntry {
                                    html,
                                    timestamp: std::time::Instant::now(),
                                },
                            );
                            info!(
                                "Cache refreshed for {} in {:?}",
                                cache_key,
                                query_start.elapsed()
                            );
                        }
                        Err(e) => {
                            error!("Failed to refresh cache for {}: {}", cache_key, e);
                        }
                    }

                    // Add a small delay between queries to prevent memory spikes
                    tokio::time::sleep(Duration::from_millis(500)).await;
                } else {
                    error!("Invalid network: {}", network_str);
                }
            }
        }
    }

    // Populate leaderboard cache for ZEC only
    if let Some(network) = SafeNetwork::from_str("zec") {
        let cache_key = "zec-leaderboard".to_string();
        let query_start = std::time::Instant::now();

        let result = fetch_and_render_leaderboard(&worker, &network).await;
        match result {
            Ok(html) => {
                let mut cache = worker.cache.write().await;
                cache.insert(
                    cache_key.clone(),
                    CacheEntry {
                        html,
                        timestamp: std::time::Instant::now(),
                    },
                );
                info!(
                    "Cache refreshed for {} in {:?}",
                    cache_key,
                    query_start.elapsed()
                );
            }
            Err(e) => {
                error!("Failed to refresh cache for {}: {}", cache_key, e);
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    info!(
        "Initial cache population completed in {:?}",
        cycle_start.elapsed()
    );

    // Then refresh periodically
    let mut interval = interval(Duration::from_secs(refresh_interval_secs));
    loop {
        interval.tick().await;

        info!("Starting cache refresh cycle");
        let cycle_start = std::time::Instant::now();

        for network_str in &networks {
            for &hide_community in &hide_community_options {
                for &tor_only in &tor_only_options {
                    let cache_key = format!("{}-{}-{}", network_str, hide_community, tor_only);

                    if let Some(network) = SafeNetwork::from_str(network_str) {
                        let query_start = std::time::Instant::now();

                        let result = fetch_and_render_network_status(
                            &worker,
                            &network,
                            hide_community,
                            tor_only,
                        )
                        .await;
                        match result {
                            Ok(html) => {
                                let mut cache = worker.cache.write().await;
                                cache.insert(
                                    cache_key.clone(),
                                    CacheEntry {
                                        html,
                                        timestamp: std::time::Instant::now(),
                                    },
                                );
                                info!(
                                    "Cache refreshed for {} in {:?}",
                                    cache_key,
                                    query_start.elapsed()
                                );
                            }
                            Err(e) => {
                                error!("Failed to refresh cache for {}: {}", cache_key, e);
                                // Keep old cache if refresh fails - don't remove it
                            }
                        }

                        // Add a small delay between queries to prevent memory spikes
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    } else {
                        error!("Invalid network: {}", network_str);
                    }
                }
            }
        }

        // Refresh leaderboard cache for ZEC only
        if let Some(network) = SafeNetwork::from_str("zec") {
            let cache_key = "zec-leaderboard".to_string();
            let query_start = std::time::Instant::now();

            let result = fetch_and_render_leaderboard(&worker, &network).await;
            match result {
                Ok(html) => {
                    let mut cache = worker.cache.write().await;
                    cache.insert(
                        cache_key.clone(),
                        CacheEntry {
                            html,
                            timestamp: std::time::Instant::now(),
                        },
                    );
                    info!(
                        "Cache refreshed for {} in {:?}",
                        cache_key,
                        query_start.elapsed()
                    );
                }
                Err(e) => {
                    error!("Failed to refresh cache for {}: {}", cache_key, e);
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        info!(
            "Cache refresh cycle completed in {:?}",
            cycle_start.elapsed()
        );
    }
}

/// Run the web server.
///
/// This is the main entry point for the web service.
pub async fn run() -> std::io::Result<()> {
    let http_client = reqwest::Client::builder()
        // Hard cap request duration so cache refreshes can't hang forever if ClickHouse stalls
        .timeout(std::time::Duration::from_secs(10))
        .pool_idle_timeout(std::time::Duration::from_secs(300))
        .pool_max_idle_per_host(32)
        .tcp_keepalive(std::time::Duration::from_secs(60))
        .build()
        .expect("Failed to create HTTP client");

    let config = Config::from_env().expect("Failed to load config from environment");

    // Initialize cache
    let cache: PageCache = Arc::new(RwLock::new(HashMap::new()));

    let worker = Worker {
        clickhouse: ClickhouseConfig::from_env(),
        http_client,
        config,
        cache: cache.clone(),
    };

    info!("🚀 Starting server at http://0.0.0.0:8080");
    info!("📦 Cache will refresh every 10 seconds");

    // Clone worker for the background cache refresh task
    let worker_for_cache = worker.clone();

    HttpServer::new(move || {
        // Clone worker for the cache task - we do this inside the closure
        // so it runs on the Actix runtime
        let worker_cache = worker_for_cache.clone();

        // Spawn the cache refresh task on first App creation
        // Using a static flag to ensure we only spawn once
        use std::sync::atomic::{AtomicBool, Ordering};
        static CACHE_TASK_STARTED: AtomicBool = AtomicBool::new(false);

        if !CACHE_TASK_STARTED.swap(true, Ordering::SeqCst) {
            actix_web::rt::spawn(async move {
                cache_refresh_task(worker_cache).await;
            });
        }

        App::new()
            .wrap(Logger::new("\"%r\" %s %b %Ts"))
            .app_data(web::Data::new(worker.clone()))
            .service(fs::Files::new("/static", "./static"))
            .service(root)
            .service(network_status)
            .service(leaderboard)
            .service(server_detail)
            .service(network_api)
            .service(get_jobs)
            .service(post_results)
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
    if let Some(naive_str) = clean_timestamp.strip_suffix('Z') {
        // Try parsing with different nanosecond formats
        let formats = [
            "%Y-%m-%dT%H:%M:%S%.f",
            "%Y-%m-%dT%H:%M:%S%.9f",
            "%Y-%m-%dT%H:%M:%S%.6f",
            "%Y-%m-%dT%H:%M:%S%.3f",
        ];

        for format in &formats {
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(naive_str, format) {
                return Some(
                    DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)
                        .with_timezone(&FixedOffset::east_opt(0).unwrap()),
                );
            }
        }
    }

    // Fallback to standard RFC3339 parsing
    DateTime::parse_from_rfc3339(clean_timestamp).ok()
}

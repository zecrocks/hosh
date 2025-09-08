use nostr_sdk::prelude::*;
use serde_json::Value;
use std::env;
use std::time::Duration;
use tokio::signal;
use chrono::Duration as ChronoDuration;
use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
enum ApiHealth {
    Healthy,
    Empty,
    Error,
    StaleChecks, // New state for when checks are too old
}

#[derive(Debug, Clone)]
struct Server {
    online: bool,
}

#[derive(Debug, Clone)]
struct ApiStatus {
    servers: Vec<Server>,
}

#[derive(Debug, Clone)]
struct HtmlServerInfo {
    hostname: String,
    last_checked: String, // e.g., "4m 21s", "1h 30m", "2d 5h"
}

impl ApiStatus {
    fn from_json(json: Value) -> Result<Self, Box<dyn std::error::Error>> {
        let servers_array = json["servers"]
            .as_array()
            .ok_or("No 'servers' array found in JSON")?;

        let servers: Vec<Server> = servers_array
            .iter()
            .map(|server| {
                Ok(Server {
                    online: server["online"]
                        .as_bool()
                        .ok_or("Missing online status")?,
                })
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;

        Ok(ApiStatus { servers })
    }

    fn online_count(&self) -> usize {
        self.servers.iter().filter(|s| s.online).count()
    }

    fn total_count(&self) -> usize {
        self.servers.len()
    }

    fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }

    fn get_health_status(&self) -> ApiHealth {
        if self.is_empty() {
            ApiHealth::Empty
        } else {
            ApiHealth::Healthy
        }
    }
}

// Parse "Last Checked" time strings like "4m 21s", "1h 30m", "2d 5h"
fn parse_last_checked_time(time_str: &str) -> Option<ChronoDuration> {
    let time_str = time_str.trim();
    
    // Handle "Just now" or very recent times
    if time_str == "Just now" || time_str == "0s" {
        return Some(ChronoDuration::seconds(0));
    }
    
    // Parse patterns like "4m 21s", "1h 30m", "2d 5h"
    let mut total_seconds = 0i64;
    
    // Use regex to extract numbers before each time unit
    let time_regex = Regex::new(r"(\d+)d|(\d+)h|(\d+)m|(\d+)s").unwrap();
    
    for cap in time_regex.captures_iter(time_str) {
        if let Some(days) = cap.get(1) {
            if let Ok(d) = days.as_str().parse::<i64>() {
                total_seconds += d * 24 * 60 * 60;
            }
        }
        if let Some(hours) = cap.get(2) {
            if let Ok(h) = hours.as_str().parse::<i64>() {
                total_seconds += h * 60 * 60;
            }
        }
        if let Some(minutes) = cap.get(3) {
            if let Ok(m) = minutes.as_str().parse::<i64>() {
                total_seconds += m * 60;
            }
        }
        if let Some(seconds) = cap.get(4) {
            if let Ok(s) = seconds.as_str().parse::<i64>() {
                total_seconds += s;
            }
        }
    }
    
    if total_seconds > 0 {
        Some(ChronoDuration::seconds(total_seconds))
    } else {
        None
    }
}

// Parse HTML and extract server information
fn parse_html_servers(html: &str) -> Result<Vec<HtmlServerInfo>, Box<dyn std::error::Error>> {
    let mut servers = Vec::new();
    
    // Regex to match table rows with server info
    // Updated to match the actual HTML structure: Server, Block Height, Status, Uptime, Version, Last Checked, USA Ping
    let row_regex = Regex::new(r#"<tr[^>]*>\s*<td><a[^>]*>([^<]+)</a></td>\s*<td[^>]*>[^<]*</td>\s*<td[^>]*>[^<]*</td>\s*<td[^>]*>[^<]*</td>\s*<td[^>]*>[^<]*</td>\s*<td>([^<]+)</td>\s*<td[^>]*>[^<]*</td>\s*</tr>"#)?;
    
    for cap in row_regex.captures_iter(html) {
        if cap.len() >= 3 {
            let hostname = cap[1].trim().to_string();
            let last_checked = cap[2].trim().to_string();
            
            servers.push(HtmlServerInfo {
                hostname,
                last_checked,
            });
        }
    }
    
    Ok(servers)
}

// Check if checks are stale (too old)
fn check_if_checks_are_stale(servers: &[HtmlServerInfo], max_age_minutes: i64) -> bool {
    if servers.is_empty() {
        return true; // No servers means stale
    }
    
    let max_age_duration = ChronoDuration::minutes(max_age_minutes);
    
    // Find the youngest check among all servers
    let youngest_check = servers.iter()
        .filter_map(|server| parse_last_checked_time(&server.last_checked))
        .min();
    
    match youngest_check {
        Some(duration) => {
            // If the youngest check is older than the threshold, consider it stale
            duration > max_age_duration
        }
        None => {
            // If we can't parse any times, assume it's stale
            true
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_app().await
}

async fn run_app() -> Result<(), Box<dyn std::error::Error>> {
    println!("Nostr Alert Monitoring System Starting...");

    // Check if user wants to generate a new private key
    let should_generate = env::var("GENERATE_KEYS")
        .unwrap_or_else(|_| "false".to_string())
        .to_lowercase() == "true";
    
    // Generate or load keys
    let keys = if should_generate {
        let keys = Keys::generate();
        println!("✅ Generated new keys:");
        println!("   Public key: {}", keys.public_key().to_bech32()?);
        println!("   Private key: {}", keys.secret_key().to_bech32()?);
        println!("   Save these keys for future use!");
        println!("   Set HOSH_PRIV_KEY environment variable with the private key above.");
        keys
    } else {
        // Load private key from environment
        match env::var("HOSH_PRIV_KEY") {
            Ok(private_key) => {
                match Keys::parse(&private_key) {
                    Ok(keys) => keys,
                    Err(e) => {
                        eprintln!("❌ Error: Invalid HOSH_PRIV_KEY format: {}", e);
                        eprintln!("   Make sure it's a valid nsec format");
                        eprintln!("   Set GENERATE_KEYS=true to generate a new keypair");
                        eprintln!("Exiting...");
                        std::process::exit(1);
                    }
                }
            }
            Err(_) => {
                eprintln!("❌ Error: HOSH_PRIV_KEY environment variable is required");
                eprintln!("   Example: export HOSH_PRIV_KEY=\"nsec1your_private_key_here\"");
                eprintln!("   Or set it in your docker-compose environment variables");
                eprintln!("   Set GENERATE_KEYS=true to generate a new keypair");
                eprintln!("Exiting...");
                std::process::exit(1);
            }
        }
    };

    // Load configuration from environment variables
    let admin_pubkey = match env::var("ADMIN_PUB_KEY") {
        Ok(key) => key,
        Err(_) => {
            eprintln!("❌ Error: ADMIN_PUB_KEY environment variable is required");
            eprintln!("   Example: export ADMIN_PUB_KEY=\"npub1your_admin_public_key_here\"");
            eprintln!("Exiting...");
            std::process::exit(1);
        }
    };
    
    let admin_pubkey = match PublicKey::parse(&admin_pubkey) {
        Ok(key) => key,
        Err(e) => {
            eprintln!("❌ Error: Invalid ADMIN_PUB_KEY format: {}", e);
            eprintln!("   Make sure it's a valid npub format");
            eprintln!("Exiting...");
            std::process::exit(1);
        }
    };
    
    // HTML URLs for stale check detection
    let zec_html_url = env::var("ZEC_HTML_URL")
        .unwrap_or_else(|_| "https://hosh.zec.rocks/zec".to_string());
    
    let btc_html_url = env::var("BTC_HTML_URL")
        .unwrap_or_else(|_| "https://hosh.zec.rocks/btc".to_string());
    
    // JSON API URLs for empty server list detection
    let zec_api_url = env::var("ZEC_API_URL")
        .unwrap_or_else(|_| "https://hosh.zec.rocks/api/v0/zec.json".to_string());
    
    let btc_api_url = env::var("BTC_API_URL")
        .unwrap_or_else(|_| "https://hosh.zec.rocks/api/v0/btc.json".to_string());
    
    let check_interval = env::var("CHECK_INTERVAL_SECONDS")
        .unwrap_or_else(|_| "60".to_string())
        .parse::<u64>()
        .unwrap_or(60);

    let max_check_age_minutes = env::var("MAX_CHECK_AGE_MINUTES")
        .unwrap_or_else(|_| "10".to_string())
        .parse::<i64>()
        .unwrap_or(10);

    println!("Monitoring ZEC HTML: {}", zec_html_url);
    println!("Monitoring BTC HTML: {}", btc_html_url);
    println!("Monitoring ZEC API: {}", zec_api_url);
    println!("Monitoring BTC API: {}", btc_api_url);
    println!("Will DM admin: {}", admin_pubkey.to_bech32()?);
    println!("Check interval: {} seconds", check_interval);
    println!("Max check age: {} minutes", max_check_age_minutes);

    // Create a client
    let client = Client::new(keys);
    
    // Add relays
    let relays = vec![
        "wss://relay.damus.io",
        "wss://nostr.wine", 
        "wss://relay.rip"
    ];
    
    for relay in relays {
        client.add_relay(relay).await?;
    }
    client.connect().await;

    // Only set metadata if we generated new keys (for discovery purposes)
    if should_generate {
        // Set metadata
        let metadata = Metadata::new()
            .name("hosh-nostr-alert")
            .display_name("Hosh Nostr Alert Monitor")
            .website(Url::parse("https://hosh.zec.rocks")?);
        client.set_metadata(&metadata).await?;
        println!("[{}] ✅ Set metadata for new keypair", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
    }

    println!("[{}] Connected to relays. Starting monitoring loop...", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
    println!("[{}] Press Ctrl+C to stop the monitoring service.", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));

    // Track previous states to avoid spam - start with None to avoid false recovery alerts
    let mut zec_previous_state: Option<ApiHealth> = None;
    let mut btc_previous_state: Option<ApiHealth> = None;

    // Monitoring loop with signal handling
    loop {
        // Check ZEC - both HTML (for stale checks) and JSON (for empty lists)
        let zec_html_result = check_html_status(&zec_html_url).await;
        let zec_json_result = check_json_status(&zec_api_url).await;
        
        let zec_current_state = match (&zec_html_result, &zec_json_result) {
            (Ok(html_servers), Ok(json_status)) => {
                // Check for empty server list first (critical)
                if json_status.is_empty() {
                    ApiHealth::Empty
                } else if check_if_checks_are_stale(html_servers, max_check_age_minutes) {
                    ApiHealth::StaleChecks
                } else {
                    ApiHealth::Healthy
                }
            }
            (Ok(html_servers), Err(_)) => {
                // JSON failed but HTML succeeded - check for stale checks
                if check_if_checks_are_stale(html_servers, max_check_age_minutes) {
                    ApiHealth::StaleChecks
                } else {
                    ApiHealth::Healthy
                }
            }
            (Err(_), Ok(json_status)) => {
                // HTML failed but JSON succeeded - check for empty list
                if json_status.is_empty() {
                    ApiHealth::Empty
                } else {
                    ApiHealth::Healthy
                }
            }
            (Err(_), Err(_)) => {
                // Both failed
                ApiHealth::Error
            }
        };
        
        // Send alerts if we have a previous state and it's different, OR if this is the first detection of a problem
        if let Some(prev_state) = zec_previous_state {
            if zec_current_state != prev_state {
                match zec_current_state {
                    ApiHealth::Empty => {
                        println!("[{}] 🚨 CRITICAL: ZEC servers list is EMPTY!", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                        
                        // Send critical alert to admin
                        let alert_message = format!(
                            "🚨 CRITICAL ALERT: ZEC SERVERS LIST IS EMPTY!\n\nAPI URL: {}\nTime: {}\n\nThis indicates a critical failure in the ZEC monitoring system.",
                            zec_api_url,
                            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                        );
                        
                        match client.send_private_msg(admin_pubkey, alert_message, []).await {
                            Ok(_) => println!("[{}] ✅ ZEC critical alert DM sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                            Err(e) => println!("[{}] ❌ Failed to send ZEC DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                        }
                    }
                    ApiHealth::StaleChecks => {
                        let youngest_check = match &zec_html_result {
                            Ok(servers) => servers.iter()
                                .filter_map(|s| parse_last_checked_time(&s.last_checked))
                                .min()
                                .unwrap_or(ChronoDuration::minutes(0)),
                            Err(_) => ChronoDuration::minutes(0),
                        };
                        
                        println!("[{}] 🚨 WARNING: ZEC checks are STALE! Youngest check: {} minutes", 
                            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), 
                            youngest_check.num_minutes());
                        
                        // Send stale checks alert to admin
                        let alert_message = format!(
                            "🚨 WARNING: ZEC CHECKS ARE STALE!\n\nHTML URL: {}\nTime: {}\nYoungest check: {} minutes\nMax allowed age: {} minutes\n\nThis indicates the monitoring system may have stopped working.",
                            zec_html_url,
                            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                            youngest_check.num_minutes(),
                            max_check_age_minutes
                        );
                        
                        match client.send_private_msg(admin_pubkey, alert_message, []).await {
                            Ok(_) => println!("[{}] ✅ ZEC stale checks alert DM sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                            Err(e) => println!("[{}] ❌ Failed to send ZEC stale DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                        }
                    }
                    ApiHealth::Healthy => {
                        let total_servers = match &zec_json_result {
                            Ok(status) => status.total_count(),
                            Err(_) => 0,
                        };
                        println!("[{}] ✅ ZEC recovered - {} servers found", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), total_servers);
                        
                        // Send recovery notification
                        let recovery_message = format!(
                            "✅ ZEC RECOVERED\n\nAPI URL: {}\nHTML URL: {}\nTime: {}\nStatus: {} servers found\n\nThe ZEC monitoring system is back online.",
                            zec_api_url,
                            zec_html_url,
                            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                            total_servers
                        );
                        
                        match client.send_private_msg(admin_pubkey, recovery_message, []).await {
                            Ok(_) => println!("[{}] ✅ ZEC recovery notification sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                            Err(e) => println!("[{}] ❌ Failed to send ZEC recovery DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                        }
                    }
                    ApiHealth::Error => {
                        println!("[{}] 🚨 ERROR: Both ZEC HTML and JSON are unreachable", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                        
                        // Send error alert to admin
                        let error_message = format!(
                            "🚨 ZEC MONITORING ERROR\n\nHTML URL: {}\nAPI URL: {}\nTime: {}\n\nBoth HTML and JSON endpoints are unreachable.",
                            zec_html_url,
                            zec_api_url,
                            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                        );
                        
                        match client.send_private_msg(admin_pubkey, error_message, []).await {
                            Ok(_) => println!("[{}] ✅ ZEC error alert DM sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                            Err(e) => println!("[{}] ❌ Failed to send ZEC error DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                        }
                    }
                }
            }
        } else {
            // First check - send alert if we detect a problem immediately
            match zec_current_state {
                ApiHealth::Empty => {
                    println!("[{}] 🚨 CRITICAL: ZEC servers list is EMPTY! (first check)", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                    
                    let alert_message = format!(
                        "🚨 CRITICAL ALERT: ZEC SERVERS LIST IS EMPTY!\n\nAPI URL: {}\nTime: {}\n\nThis indicates a critical failure in the ZEC monitoring system.",
                        zec_api_url,
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                    );
                    
                    match client.send_private_msg(admin_pubkey, alert_message, []).await {
                        Ok(_) => println!("[{}] ✅ ZEC critical alert DM sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                        Err(e) => println!("[{}] ❌ Failed to send ZEC DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                    }
                }
                ApiHealth::StaleChecks => {
                    let youngest_check = match &zec_html_result {
                        Ok(servers) => servers.iter()
                            .filter_map(|s| parse_last_checked_time(&s.last_checked))
                            .min()
                            .unwrap_or(ChronoDuration::minutes(0)),
                        Err(_) => ChronoDuration::minutes(0),
                    };
                    
                    println!("[{}] 🚨 WARNING: ZEC checks are STALE! (first check) Youngest check: {} minutes", 
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), 
                        youngest_check.num_minutes());
                    
                    let alert_message = format!(
                        "🚨 WARNING: ZEC CHECKS ARE STALE!\n\nHTML URL: {}\nTime: {}\nYoungest check: {} minutes\nMax allowed age: {} minutes\n\nThis indicates the monitoring system may have stopped working.",
                        zec_html_url,
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                        youngest_check.num_minutes(),
                        max_check_age_minutes
                    );
                    
                    match client.send_private_msg(admin_pubkey, alert_message, []).await {
                        Ok(_) => println!("[{}] ✅ ZEC stale checks alert DM sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                        Err(e) => println!("[{}] ❌ Failed to send ZEC stale DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                    }
                }
                ApiHealth::Error => {
                    println!("[{}] 🚨 ERROR: Both ZEC HTML and JSON are unreachable (first check)", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                    
                    let error_message = format!(
                        "🚨 ZEC MONITORING ERROR\n\nHTML URL: {}\nAPI URL: {}\nTime: {}\n\nBoth HTML and JSON endpoints are unreachable.",
                        zec_html_url,
                        zec_api_url,
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                    );
                    
                    match client.send_private_msg(admin_pubkey, error_message, []).await {
                        Ok(_) => println!("[{}] ✅ ZEC error alert DM sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                        Err(e) => println!("[{}] ❌ Failed to send ZEC error DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                    }
                }
                ApiHealth::Healthy => {
                    // Don't send recovery alert on first check if healthy
                    println!("[{}] ✅ ZEC healthy on first check", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                }
            }
        }
        
        // Update previous state
        zec_previous_state = Some(zec_current_state.clone());
        
        // Log current status (without sending alerts)
        match zec_current_state {
            ApiHealth::Healthy => {
                let total_servers = match &zec_json_result {
                    Ok(status) => status.total_count(),
                    Err(_) => 0,
                };
                println!("[{}] ✅ ZEC healthy - {} servers found", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), total_servers);
            }
            ApiHealth::Empty => {
                println!("[{}] 🚨 ZEC servers list still empty (no new alert sent)", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
            }
            ApiHealth::StaleChecks => {
                let youngest_check = match &zec_html_result {
                    Ok(servers) => servers.iter()
                        .filter_map(|s| parse_last_checked_time(&s.last_checked))
                        .min()
                        .unwrap_or(ChronoDuration::minutes(0)),
                    Err(_) => ChronoDuration::minutes(0),
                };
                println!("[{}] 🚨 ZEC checks still stale - youngest: {} minutes (no new alert sent)", 
                    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), 
                    youngest_check.num_minutes());
            }
            ApiHealth::Error => {
                println!("[{}] 🚨 ZEC still unreachable (no new alert sent)", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
            }
        }

        // Check BTC - both HTML (for stale checks) and JSON (for empty lists)
        let btc_html_result = check_html_status(&btc_html_url).await;
        let btc_json_result = check_json_status(&btc_api_url).await;
        
        let btc_current_state = match (&btc_html_result, &btc_json_result) {
            (Ok(html_servers), Ok(json_status)) => {
                // Check for empty server list first (critical)
                if json_status.is_empty() {
                    ApiHealth::Empty
                } else if check_if_checks_are_stale(html_servers, max_check_age_minutes) {
                    ApiHealth::StaleChecks
                } else {
                    ApiHealth::Healthy
                }
            }
            (Ok(html_servers), Err(_)) => {
                // JSON failed but HTML succeeded - check for stale checks
                if check_if_checks_are_stale(html_servers, max_check_age_minutes) {
                    ApiHealth::StaleChecks
                } else {
                    ApiHealth::Healthy
                }
            }
            (Err(_), Ok(json_status)) => {
                // HTML failed but JSON succeeded - check for empty list
                if json_status.is_empty() {
                    ApiHealth::Empty
                } else {
                    ApiHealth::Healthy
                }
            }
            (Err(_), Err(_)) => {
                // Both failed
                ApiHealth::Error
            }
        };
        
        // Send alerts if we have a previous state and it's different, OR if this is the first detection of a problem
        if let Some(prev_state) = btc_previous_state {
            if btc_current_state != prev_state {
                match btc_current_state {
                    ApiHealth::Empty => {
                        println!("[{}] 🚨 CRITICAL: BTC servers list is EMPTY!", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                        
                        // Send critical alert to admin
                        let alert_message = format!(
                            "🚨 CRITICAL ALERT: BTC SERVERS LIST IS EMPTY!\n\nAPI URL: {}\nTime: {}\n\nThis indicates a critical failure in the BTC monitoring system.",
                            btc_api_url,
                            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                        );
                        
                        match client.send_private_msg(admin_pubkey, alert_message, []).await {
                            Ok(_) => println!("[{}] ✅ BTC critical alert DM sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                            Err(e) => println!("[{}] ❌ Failed to send BTC DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                        }
                    }
                    ApiHealth::StaleChecks => {
                        let youngest_check = match &btc_html_result {
                            Ok(servers) => servers.iter()
                                .filter_map(|s| parse_last_checked_time(&s.last_checked))
                                .min()
                                .unwrap_or(ChronoDuration::minutes(0)),
                            Err(_) => ChronoDuration::minutes(0),
                        };
                        
                        println!("[{}] 🚨 WARNING: BTC checks are STALE! Youngest check: {} minutes", 
                            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), 
                            youngest_check.num_minutes());
                        
                        // Send stale checks alert to admin
                        let alert_message = format!(
                            "🚨 WARNING: BTC CHECKS ARE STALE!\n\nHTML URL: {}\nTime: {}\nYoungest check: {} minutes\nMax allowed age: {} minutes\n\nThis indicates the monitoring system may have stopped working.",
                            btc_html_url,
                            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                            youngest_check.num_minutes(),
                            max_check_age_minutes
                        );
                        
                        match client.send_private_msg(admin_pubkey, alert_message, []).await {
                            Ok(_) => println!("[{}] ✅ BTC stale checks alert DM sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                            Err(e) => println!("[{}] ❌ Failed to send BTC stale DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                        }
                    }
                    ApiHealth::Healthy => {
                        let total_servers = match &btc_json_result {
                            Ok(status) => status.total_count(),
                            Err(_) => 0,
                        };
                        println!("[{}] ✅ BTC recovered - {} servers found", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), total_servers);
                        
                        // Send recovery notification
                        let recovery_message = format!(
                            "✅ BTC RECOVERED\n\nAPI URL: {}\nHTML URL: {}\nTime: {}\nStatus: {} servers found\n\nThe BTC monitoring system is back online.",
                            btc_api_url,
                            btc_html_url,
                            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                            total_servers
                        );
                        
                        match client.send_private_msg(admin_pubkey, recovery_message, []).await {
                            Ok(_) => println!("[{}] ✅ BTC recovery notification sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                            Err(e) => println!("[{}] ❌ Failed to send BTC recovery DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                        }
                    }
                    ApiHealth::Error => {
                        println!("[{}] 🚨 ERROR: Both BTC HTML and JSON are unreachable", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                        
                        // Send error alert to admin
                        let error_message = format!(
                            "🚨 BTC MONITORING ERROR\n\nHTML URL: {}\nAPI URL: {}\nTime: {}\n\nBoth HTML and JSON endpoints are unreachable.",
                            btc_html_url,
                            btc_api_url,
                            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                        );
                        
                        match client.send_private_msg(admin_pubkey, error_message, []).await {
                            Ok(_) => println!("[{}] ✅ BTC error alert DM sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                            Err(e) => println!("[{}] ❌ Failed to send BTC error DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                        }
                    }
                }
            }
        } else {
            // First check - send alert if we detect a problem immediately
            match btc_current_state {
                ApiHealth::Empty => {
                    println!("[{}] 🚨 CRITICAL: BTC servers list is EMPTY! (first check)", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                    
                    let alert_message = format!(
                        "🚨 CRITICAL ALERT: BTC SERVERS LIST IS EMPTY!\n\nAPI URL: {}\nTime: {}\n\nThis indicates a critical failure in the BTC monitoring system.",
                        btc_api_url,
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                    );
                    
                    match client.send_private_msg(admin_pubkey, alert_message, []).await {
                        Ok(_) => println!("[{}] ✅ BTC critical alert DM sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                        Err(e) => println!("[{}] ❌ Failed to send BTC DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                    }
                }
                ApiHealth::StaleChecks => {
                    let youngest_check = match &btc_html_result {
                        Ok(servers) => servers.iter()
                            .filter_map(|s| parse_last_checked_time(&s.last_checked))
                            .min()
                            .unwrap_or(ChronoDuration::minutes(0)),
                        Err(_) => ChronoDuration::minutes(0),
                    };
                    
                    println!("[{}] 🚨 WARNING: BTC checks are STALE! (first check) Youngest check: {} minutes", 
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), 
                        youngest_check.num_minutes());
                    
                    let alert_message = format!(
                        "🚨 WARNING: BTC CHECKS ARE STALE!\n\nHTML URL: {}\nTime: {}\nYoungest check: {} minutes\nMax allowed age: {} minutes\n\nThis indicates the monitoring system may have stopped working.",
                        btc_html_url,
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                        youngest_check.num_minutes(),
                        max_check_age_minutes
                    );
                    
                    match client.send_private_msg(admin_pubkey, alert_message, []).await {
                        Ok(_) => println!("[{}] ✅ BTC stale checks alert DM sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                        Err(e) => println!("[{}] ❌ Failed to send BTC stale DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                    }
                }
                ApiHealth::Error => {
                    println!("[{}] 🚨 ERROR: Both BTC HTML and JSON are unreachable (first check)", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                    
                    let error_message = format!(
                        "🚨 BTC MONITORING ERROR\n\nHTML URL: {}\nAPI URL: {}\nTime: {}\n\nBoth HTML and JSON endpoints are unreachable.",
                        btc_html_url,
                        btc_api_url,
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                    );
                    
                    match client.send_private_msg(admin_pubkey, error_message, []).await {
                        Ok(_) => println!("[{}] ✅ BTC error alert DM sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                        Err(e) => println!("[{}] ❌ Failed to send BTC error DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                    }
                }
                ApiHealth::Healthy => {
                    // Don't send recovery alert on first check if healthy
                    println!("[{}] ✅ BTC healthy on first check", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                }
            }
        }
        
        // Update previous state
        btc_previous_state = Some(btc_current_state.clone());
        
        // Log current status (without sending alerts)
        match btc_current_state {
            ApiHealth::Healthy => {
                let total_servers = match &btc_json_result {
                    Ok(status) => status.total_count(),
                    Err(_) => 0,
                };
                println!("[{}] ✅ BTC healthy - {} servers found", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), total_servers);
            }
            ApiHealth::Empty => {
                println!("[{}] 🚨 BTC servers list still empty (no new alert sent)", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
            }
            ApiHealth::StaleChecks => {
                let youngest_check = match &btc_html_result {
                    Ok(servers) => servers.iter()
                        .filter_map(|s| parse_last_checked_time(&s.last_checked))
                        .min()
                        .unwrap_or(ChronoDuration::minutes(0)),
                    Err(_) => ChronoDuration::minutes(0),
                };
                println!("[{}] 🚨 BTC checks still stale - youngest: {} minutes (no new alert sent)", 
                    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), 
                    youngest_check.num_minutes());
            }
            ApiHealth::Error => {
                println!("[{}] 🚨 BTC still unreachable (no new alert sent)", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
            }
        }

        // Wait before next check with signal handling
        match tokio::time::timeout(Duration::from_secs(check_interval), signal::ctrl_c()).await {
            Ok(Ok(())) => {
                println!("[{}] 🛑 Shutdown signal received. Disconnecting from relays...", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                client.disconnect().await;
                println!("[{}] ✅ Disconnected from relays. Exiting gracefully.", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                std::process::exit(0);
            }
            Ok(Err(_)) => {
                println!("[{}] 🛑 Shutdown signal received. Disconnecting from relays...", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                client.disconnect().await;
                println!("[{}] ✅ Disconnected from relays. Exiting gracefully.", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                std::process::exit(0);
            }
            Err(_) => {
                // Timeout occurred, continue to next check
            }
        }
    }
}

async fn check_html_status(url: &str) -> Result<Vec<HtmlServerInfo>, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client.get(url).timeout(Duration::from_secs(10)).send().await?;
    
    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()).into());
    }
    
    let html = response.text().await?;
    parse_html_servers(&html)
}

async fn check_json_status(url: &str) -> Result<ApiStatus, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client.get(url).timeout(Duration::from_secs(10)).send().await?;
    
    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()).into());
    }
    
    let json: Value = response.json().await?;
    ApiStatus::from_json(json)
}

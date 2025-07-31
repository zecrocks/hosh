use nostr_sdk::prelude::*;
use serde_json::Value;
use std::env;
use std::time::Duration;
use tokio::signal;

#[derive(Debug, Clone, PartialEq)]
enum ApiHealth {
    Healthy,
    Empty,
    Error,
}

#[derive(Debug)]
struct Server {
    online: bool,
}

#[derive(Debug)]
struct ApiStatus {
    servers: Vec<Server>,
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
    
    let zec_api_url = env::var("ZEC_API_URL")
        .unwrap_or_else(|_| "https://hosh.zec.rocks/api/v0/zec.json".to_string());
    
    let btc_api_url = env::var("BTC_API_URL")
        .unwrap_or_else(|_| "https://hosh.zec.rocks/api/v0/btc.json".to_string());
    
    let check_interval = env::var("CHECK_INTERVAL_SECONDS")
        .unwrap_or_else(|_| "60".to_string())
        .parse::<u64>()
        .unwrap_or(60);

    println!("Monitoring ZEC API: {}", zec_api_url);
    println!("Monitoring BTC API: {}", btc_api_url);
    println!("Will DM admin: {}", admin_pubkey.to_bech32()?);
    println!("Check interval: {} seconds", check_interval);

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

    // Track previous states to avoid spam
    let mut zec_previous_state = ApiHealth::Healthy;
    let mut btc_previous_state = ApiHealth::Healthy;

    // Monitoring loop with signal handling
    loop {
        // Check ZEC API
        match check_api_status(&zec_api_url).await {
            Ok(zec_status) => {
                let current_state = zec_status.get_health_status();
                
                if current_state != zec_previous_state {
                    match current_state {
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
                        ApiHealth::Healthy => {
                            let online = zec_status.online_count();
                            let total = zec_status.total_count();
                            println!("[{}] ✅ ZEC API recovered - {}/{} servers online", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), online, total);
                            
                            // Send recovery notification
                            let recovery_message = format!(
                                "✅ ZEC API RECOVERED\n\nAPI URL: {}\nTime: {}\nStatus: {}/{} servers online\n\nThe ZEC monitoring system is back online.",
                                zec_api_url,
                                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                                online, total
                            );
                            
                            match client.send_private_msg(admin_pubkey, recovery_message, []).await {
                                Ok(_) => println!("[{}] ✅ ZEC recovery notification sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                                Err(e) => println!("[{}] ❌ Failed to send ZEC recovery DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                            }
                        }
                        ApiHealth::Error => {
                            // This shouldn't happen in the Ok branch, but just in case
                        }
                    }
                    zec_previous_state = current_state;
                } else {
                    // Status hasn't changed, just log the current state
                    match current_state {
                        ApiHealth::Healthy => {
                            let online = zec_status.online_count();
                            let total = zec_status.total_count();
                            println!("[{}] ✅ ZEC API healthy - {}/{} servers online", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), online, total);
                        }
                        ApiHealth::Empty => {
                            println!("[{}] 🚨 ZEC servers list still empty (no new alert sent)", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                        }
                        ApiHealth::Error => {
                            // This shouldn't happen
                        }
                    }
                }
            }
            Err(e) => {
                let current_state = ApiHealth::Error;
                
                if current_state != zec_previous_state {
                    println!("[{}] 🚨 ERROR CHECKING ZEC API: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e);
                    
                    // Send error alert to admin
                    let error_message = format!(
                        "🚨 ZEC API MONITORING ERROR\n\nAPI URL: {}\nError: {}\nTime: {}",
                        zec_api_url,
                        e,
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                    );
                    
                    match client.send_private_msg(admin_pubkey, error_message, []).await {
                        Ok(_) => println!("[{}] ✅ ZEC error alert DM sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                        Err(e) => println!("[{}] ❌ Failed to send ZEC error DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                    }
                    zec_previous_state = current_state;
                } else {
                    println!("[{}] 🚨 ZEC API still unreachable (no new alert sent)", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                }
            }
        }

        // Check BTC API
        match check_api_status(&btc_api_url).await {
            Ok(btc_status) => {
                let current_state = btc_status.get_health_status();
                
                if current_state != btc_previous_state {
                    match current_state {
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
                        ApiHealth::Healthy => {
                            let online = btc_status.online_count();
                            let total = btc_status.total_count();
                            println!("[{}] ✅ BTC API recovered - {}/{} servers online", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), online, total);
                            
                            // Send recovery notification
                            let recovery_message = format!(
                                "✅ BTC API RECOVERED\n\nAPI URL: {}\nTime: {}\nStatus: {}/{} servers online\n\nThe BTC monitoring system is back online.",
                                btc_api_url,
                                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                                online, total
                            );
                            
                            match client.send_private_msg(admin_pubkey, recovery_message, []).await {
                                Ok(_) => println!("[{}] ✅ BTC recovery notification sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                                Err(e) => println!("[{}] ❌ Failed to send BTC recovery DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                            }
                        }
                        ApiHealth::Error => {
                            // This shouldn't happen in the Ok branch, but just in case
                        }
                    }
                    btc_previous_state = current_state;
                } else {
                    // Status hasn't changed, just log the current state
                    match current_state {
                        ApiHealth::Healthy => {
                            let online = btc_status.online_count();
                            let total = btc_status.total_count();
                            println!("[{}] ✅ BTC API healthy - {}/{} servers online", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), online, total);
                        }
                        ApiHealth::Empty => {
                            println!("[{}] 🚨 BTC servers list still empty (no new alert sent)", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                        }
                        ApiHealth::Error => {
                            // This shouldn't happen
                        }
                    }
                }
            }
            Err(e) => {
                let current_state = ApiHealth::Error;
                
                if current_state != btc_previous_state {
                    println!("[{}] 🚨 ERROR CHECKING BTC API: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e);
                    
                    // Send error alert to admin
                    let error_message = format!(
                        "🚨 BTC API MONITORING ERROR\n\nAPI URL: {}\nError: {}\nTime: {}",
                        btc_api_url,
                        e,
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                    );
                    
                    match client.send_private_msg(admin_pubkey, error_message, []).await {
                        Ok(_) => println!("[{}] ✅ BTC error alert DM sent to admin", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
                        Err(e) => println!("[{}] ❌ Failed to send BTC error DM: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), e),
                    }
                    btc_previous_state = current_state;
                } else {
                    println!("[{}] 🚨 BTC API still unreachable (no new alert sent)", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                }
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

async fn check_api_status(url: &str) -> Result<ApiStatus, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client.get(url).timeout(Duration::from_secs(10)).send().await?;
    
    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()).into());
    }
    
    let json: Value = response.json().await?;
    ApiStatus::from_json(json)
}

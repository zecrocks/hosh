use scraper::{Html, Selector};
use std::collections::HashMap;
use crate::types::BlockchainInfo;
use reqwest::Proxy;
use std::env;
use std::time::{Duration, Instant};
use std::error::Error;

pub async fn get_blockchain_data(client: &reqwest::Client, onion_url: &str) -> Result<HashMap<String, BlockchainInfo>, Box<dyn Error + Send + Sync>> {
    let mut blockchain_data = HashMap::new();
    
    println!("🧅 Attempting to connect to Blockchair onion site...");
    
    match tokio::time::timeout(
        Duration::from_secs(30),
        client.get(onion_url).send()
    ).await {
        Ok(response_result) => {
            match response_result {
                Ok(response) => {
                    println!("🧅 Got response from onion site with status: {}", response.status());
                    if response.status().is_success() {
                        let start_time = Instant::now();
                        let text = response.text().await?;
                        let response_time = start_time.elapsed().as_secs_f32() * 1000.0; // Convert to milliseconds
                        println!("🧅 Successfully got HTML from onion site, length: {} bytes", text.len());

                        let document = Html::parse_document(&text);
                        let card_selector = Selector::parse("a.blockchain-card").unwrap();
                        let block_selector = Selector::parse("div[data-block][data-current-value]").unwrap();

                        for card in document.select(&card_selector) {
                            if let Some(href) = card.value().attr("href") {
                                let endpoint = href
                                    .replace(&format!("{}/", onion_url), "")
                                    .replace("https://blockchair.com/", "");
                                
                                if !endpoint.is_empty() {
                                    let height = card.select(&block_selector)
                                        .next()
                                        .and_then(|el| el.value().attr("data-current-value"))
                                        .and_then(|h| h.parse::<u64>().ok());

                                    if let Some(height) = height {
                                        blockchain_data.insert(endpoint.clone(), BlockchainInfo {
                                            height: Some(height),
                                            name: endpoint.clone(),
                                            response_time_ms: response_time,
                                            extra: HashMap::new(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                },
                Err(e) => println!("🧅 Connection error to onion site: {}", e)
            }
        },
        Err(_) => println!("🧅 Timeout connecting to onion site after 30 seconds")
    }

    if blockchain_data.is_empty() {
        println!("🧅 Warning: No blockchain data retrieved from onion site");
    } else {
        println!("🧅 Retrieved data for {} chains", blockchain_data.len());
        
        // Print heights with new format
        for (chain, info) in &blockchain_data {
            if let Some(height) = info.height {
                println!("🧅 {}: {} (blockchair-onion)", chain, height);
            }
        }
    }

    Ok(blockchain_data)
}

#[allow(dead_code)]
async fn fetch_blockchain_height(client: &reqwest::Client, url: &str) -> Result<u64, Box<dyn Error + Send + Sync>> {
    // Add timeout for the entire height fetch operation
    match tokio::time::timeout(
        Duration::from_secs(30),
        async {
            let response = client.get(url).send().await?;
            
            if !response.status().is_success() {
                return Err(format!("HTTP error: {}", response.status()).into());
            }

            let html = response.text().await?;
            let document = Html::parse_document(&html);
            
            let block_selector = Selector::parse("div[data-block][data-current-value]").unwrap();
            
            if let Some(block_element) = document.select(&block_selector).next() {
                if let Some(height_str) = block_element.value().attr("data-current-value") {
                    if let Ok(height) = height_str.parse::<u64>() {
                        return Ok(height);
                    }
                }
            }

            Err("Could not find block height".into())
        }
    ).await {
        Ok(result) => result,
        Err(_) => Err("Timeout fetching blockchain height".into())
    }
}

pub fn create_client() -> Result<reqwest::Client, Box<dyn Error + Send + Sync>> {
    let tor_proxy_host = env::var("TOR_PROXY_HOST").unwrap_or_else(|_| "tor".to_string());
    let tor_proxy_port = env::var("TOR_PROXY_PORT").unwrap_or_else(|_| "9050".to_string());
    let proxy_url = format!("socks5h://{}:{}", tor_proxy_host, tor_proxy_port);
    
    println!("🧅 Setting up Tor proxy at {}", proxy_url);
    
    Ok(reqwest::Client::builder()
        .proxy(Proxy::all(&proxy_url)?)
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(Duration::from_secs(30))  // Reduced from 60 to 30
        .connect_timeout(Duration::from_secs(15))  // Reduced from 30 to 15
        .pool_idle_timeout(Duration::from_secs(45))  // Reduced from 90 to 45
        .build()?)
}

pub async fn get_blockchain_info(onion_url: &str) -> Result<HashMap<String, BlockchainInfo>, Box<dyn std::error::Error + Send + Sync>> {
    println!("🧅 Starting Blockchair onion site check");
    
    // Test Tor connectivity
    let tor_proxy_host = env::var("TOR_PROXY_HOST").unwrap_or_else(|_| "tor".to_string());
    let tor_proxy_port = env::var("TOR_PROXY_PORT").unwrap_or_else(|_| "9050".to_string());
    println!("🧅 Attempting to connect to Tor proxy at {}:{}", tor_proxy_host, tor_proxy_port);

    match create_client() {
        Ok(tor_client) => {
            println!("🧅 Successfully created Tor client");
            get_blockchain_data(&tor_client, onion_url).await
        }
        Err(e) => {
            println!("🧅 Failed to create Tor client: {}", e);
            Ok(HashMap::new())
        }
    }
} 
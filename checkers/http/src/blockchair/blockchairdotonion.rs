use scraper::{Html, Selector};
use std::collections::HashMap;
use crate::types::BlockchainInfo;
use reqwest::Proxy;
use std::env;
use std::time::Duration;
use tokio::time::sleep;
use tokio::sync::Semaphore;
use std::sync::Arc;
use std::error::Error;
use std::collections::HashSet;

pub async fn get_blockchain_data(client: &reqwest::Client) -> Result<HashMap<String, BlockchainInfo>, Box<dyn Error + Send + Sync>> {
    let mut blockchain_data = HashMap::new();
    let onion_url = "http://blkchairbknpn73cfjhevhla7rkp4ed5gg2knctvv7it4lioy22defid.onion";
    println!("🧅 Attempting to connect to Blockchair onion site...");
    
    match tokio::time::timeout(
        Duration::from_secs(5),
        client.get(onion_url).send()
    ).await {
        Ok(response_result) => {
            match response_result {
                Ok(response) => {
                    println!("🧅 Got response from onion site with status: {}", response.status());
                    if response.status().is_success() {
                        let text = response.text().await?;
                        println!("🧅 Successfully got HTML from onion site, length: {} bytes", text.len());

                        let document = Html::parse_document(&text);
                        let blockchain_selector = Selector::parse("a.blockchain-card").unwrap();
                        let name_selector = Selector::parse("span").unwrap();

                        // Limit concurrent requests
                        let semaphore = Arc::new(Semaphore::new(3));  // Only 3 concurrent requests
                        let mut handles = Vec::new();
                        let mut seen_chains = HashSet::new();

                        for blockchain in document.select(&blockchain_selector) {
                            if let Some(href) = blockchain.value().attr("href") {
                                let chain = href.trim_start_matches('/')
                                    .trim_start_matches(onion_url)
                                    .trim_start_matches('/')
                                    .to_string();
                                
                                // Skip if we've already processed this chain
                                if seen_chains.contains(&chain) || chain.is_empty() {
                                    continue;
                                }
                                seen_chains.insert(chain.clone());
                                
                                let name = blockchain
                                    .select(&name_selector)
                                    .next()
                                    .map(|el| el.text().collect::<String>())
                                    .unwrap_or_else(|| chain.clone());

                                // Skip entries with just "·" as the name
                                if name.trim() == "·" {
                                    continue;
                                }

                                println!("🧅 Found blockchain: {} ({})", name, chain);
                                
                                // Create a mapping for standardized symbols
                                let symbol = match chain.as_str() {
                                    "bitcoin" => "btc",
                                    "ethereum" => "eth",
                                    "bitcoin-cash" => "bch",
                                    "litecoin" => "ltc",
                                    "dogecoin" => "doge",
                                    "zcash" => "zec",
                                    "dash" => "dash",
                                    "monero" => "xmr",
                                    "ethereum-classic" => "etc",
                                    "cardano" => "ada",
                                    "polkadot" => "dot",
                                    "solana" => "sol",
                                    "tron" => "trx",
                                    "ripple" => "xrp",
                                    _ => chain.as_str()
                                }.to_string();

                                blockchain_data.insert(chain.clone(), BlockchainInfo {
                                    height: None,
                                    name,
                                    symbol, // Use the standardized symbol
                                    extra: HashMap::new(),
                                });

                                let full_url = format!("{}/{}", onion_url.trim_end_matches('/'), chain);
                                let client = client.clone();
                                let chain_clone = chain.clone();
                                let sem = semaphore.clone();

                                handles.push(tokio::spawn(async move {
                                    let _permit = sem.acquire().await.unwrap();
                                    
                                    // Try up to 2 times with shorter delays
                                    for attempt in 1..=2 {
                                        match fetch_blockchain_height(&client, &full_url).await {
                                            Ok(height) => return Some((chain_clone, height)),
                                            Err(e) => {
                                                println!("🧅 Attempt {} failed for {}: {} (URL: {})", 
                                                    attempt, chain_clone, e, full_url);
                                                if attempt < 2 {
                                                    sleep(Duration::from_secs(attempt)).await;
                                                    continue;
                                                }
                                            }
                                        }
                                    }
                                    None
                                }));
                            }
                        }

                        // Wait for all height fetches to complete
                        for handle in handles {
                            if let Ok(Some((chain, height))) = handle.await {
                                if let Some(info) = blockchain_data.get_mut(&chain) {
                                    info.height = Some(height);
                                    println!("🧅 Updated height for {}: {}", chain, height);
                                }
                            }
                        }
                    }
                },
                Err(e) => println!("🧅 Error connecting to onion site: {}", e)
            }
        },
        Err(_) => println!("🧅 Error: Timeout connecting to onion site after 5 seconds")
    }

    Ok(blockchain_data)
}

async fn fetch_blockchain_height(client: &reqwest::Client, url: &str) -> Result<u64, Box<dyn Error + Send + Sync>> {
    let response = client.get(url).send().await?;
    
    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()).into());
    }

    let html = response.text().await?;
    let document = Html::parse_document(&html);
    
    // Try to find the block height in the page
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

pub fn create_client() -> Result<reqwest::Client, Box<dyn Error + Send + Sync>> {
    let tor_proxy_host = env::var("TOR_PROXY_HOST").unwrap_or_else(|_| "tor".to_string());
    let tor_proxy_port = env::var("TOR_PROXY_PORT").unwrap_or_else(|_| "9050".to_string());
    let proxy_url = format!("socks5h://{}:{}", tor_proxy_host, tor_proxy_port);
    
    println!("🧅 Setting up Tor proxy at {}", proxy_url);
    
    Ok(reqwest::Client::builder()
        .proxy(Proxy::all(&proxy_url)?)
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(Duration::from_secs(10))
        .build()?)
}

pub async fn get_blockchain_info() -> Result<HashMap<String, BlockchainInfo>, Box<dyn Error + Send + Sync>> {
    println!("🧅 Starting Blockchair onion site check");
    match create_client() {
        Ok(client) => {
            println!("🧅 Successfully created Tor client");
            match get_blockchain_data(&client).await {
                Ok(data) => {
                    println!("🧅 Successfully completed onion check");
                    Ok(data)
                }
                Err(e) => {
                    println!("🧅 Error during blockchain data fetch: {}", e);
                    Ok(HashMap::new())
                }
            }
        }
        Err(e) => {
            println!("🧅 Failed to create Tor client: {}", e);
            Ok(HashMap::new())
        }
    }
} 
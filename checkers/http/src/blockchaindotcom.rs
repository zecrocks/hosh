use scraper::{Html, Selector};
use std::collections::HashMap;
use std::time::Instant;
use crate::types::BlockchainInfo;  // Update to use the shared type

// Utility function to fetch block height for a specific blockchain
async fn fetch_block_height(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<Option<(u64, f32)>, Box<dyn std::error::Error + Send + Sync>> {
    // Map the Redis key name to the correct URL path
    let url_path = match symbol {
        "bitcoin" => "btc",
        "ethereum" => "eth",
        "bitcoin-cash" => "bch",
        _ => symbol,
    };

    let url = format!("https://www.blockchain.com/explorer/blocks/{}", url_path);
    let start_time = Instant::now();
    let response = client.get(&url).send().await?;
    let response_time = start_time.elapsed().as_secs_f32() * 1000.0; // Convert to milliseconds
    
    if !response.status().is_success() {
        return Ok(None);
    }

    let html = response.text().await?;
    let document = Html::parse_document(&html);
    let block_selector = Selector::parse("div.sc-4c3a315b-2").unwrap();
    
    if let Some(block_element) = document.select(&block_selector).next() {
        let height_str = block_element.text().collect::<String>();
        if let Some(height) = height_str.trim().replace(",", "").parse::<u64>().ok() {
            return Ok(Some((height, response_time)));
        }
    }

    Ok(None)
}

// Main function to fetch block heights for BTC, ETH, and BCH
pub async fn get_blockchain_info() -> Result<HashMap<String, BlockchainInfo>, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;
    
    let mut blockchain_data = HashMap::new();
    
    // Define the supported blockchains with their URLs and display names
    let supported_chains = vec![
        ("bitcoin", "Bitcoin"),
        ("ethereum", "Ethereum"),
        ("bitcoin-cash", "Bitcoin Cash"),
    ];

    for (chain, name) in supported_chains {
        match fetch_block_height(&client, chain).await {
            Ok(Some((height, response_time))) => {
                blockchain_data.insert(chain.to_string(), BlockchainInfo {
                    height: Some(height),
                    name: name.to_string(),
                    response_time_ms: response_time,
                    extra: HashMap::new(),
                });
            },
            Ok(None) => {
                println!("Warning: Could not fetch height for {}", chain);
            },
            Err(e) => {
                println!("Error fetching {} block height: {}", chain, e);
            }
        }
    }

    Ok(blockchain_data)
} 
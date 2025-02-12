use scraper::{Html, Selector};
use std::collections::HashMap;

#[derive(Debug)]
pub struct BlockchainInfo {
    pub height: Option<u64>,
    pub name: String,
    pub symbol: String,
}

// Utility function to fetch block height for a specific blockchain
async fn fetch_block_height(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<Option<u64>, Box<dyn std::error::Error>> {
    // Map the Redis key name to the correct URL path
    let url_path = match symbol {
        "bitcoin" => "btc",
        "ethereum" => "eth",
        "bitcoin-cash" => "bch",
        _ => symbol,
    };

    let url = format!("https://www.blockchain.com/explorer/blocks/{}", url_path);
    let response = client.get(&url).send().await?;
    
    if !response.status().is_success() {
        return Ok(None);
    }

    let html = response.text().await?;
    let document = Html::parse_document(&html);
    let block_selector = Selector::parse("div.sc-4c3a315b-2").unwrap();
    
    if let Some(block_element) = document.select(&block_selector).next() {
        let height_str = block_element.text().collect::<String>();
        if let Some(height) = height_str.trim().replace(",", "").parse::<u64>().ok() {
            return Ok(Some(height));
        }
    }

    Ok(None)
}

// Main function to fetch block heights for BTC, ETH, and BCH
pub async fn get_blockchain_info() -> Result<HashMap<String, BlockchainInfo>, Box<dyn std::error::Error>> {
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

    for (symbol, name) in supported_chains {
        match fetch_block_height(&client, symbol).await {
            Ok(Some(height)) => {
                blockchain_data.insert(symbol.to_string(), BlockchainInfo {
                    height: Some(height),
                    name: name.to_string(),
                    symbol: symbol.to_string(),
                });
            },
            Ok(None) => {
                println!("Warning: Could not fetch height for {}", symbol);
            },
            Err(e) => {
                println!("Error fetching {} block height: {}", symbol, e);
            }
        }
    }

    Ok(blockchain_data)
}

// Move market data fetching to a utility module
pub mod utils {
    #[derive(Debug)]
    pub struct CryptoMarketInfo {
        pub name: String,
        pub symbol: String,
        pub price: String,
        pub change_24h: String,
        pub market_cap: String,
        pub volume_24h: String,
        pub circulating_supply: String,
    }

    pub async fn get_market_data() -> Result<Vec<CryptoMarketInfo>, Box<dyn std::error::Error>> {
        // ... (rest of the market data code)
        todo!("Moved to utils module")
    }
} 
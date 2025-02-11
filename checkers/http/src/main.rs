use reqwest;
use scraper::{Html, Selector};
use std::collections::HashMap;
use redis::Commands;
use std::env;
use serde_json::json;

#[derive(Debug)]
struct BlockchainInfo {
    height: Option<u64>,
    ticker: Option<String>,
}

async fn get_blockchain_info() -> Result<HashMap<String, BlockchainInfo>, Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;

    let url = "https://blockchair.com";
    let response = client.get(url).send().await?.text().await?;
    let document = Html::parse_document(&response);
    
    let mut blockchain_data = HashMap::new();
    
    // Selectors
    let card_selector = Selector::parse("a.blockchain-card").unwrap();
    let block_selector = Selector::parse("div[data-block][data-current-value]").unwrap();
    let ticker_selector = Selector::parse("div.color-text-secondary.flex-shrink-0").unwrap();

    for card in document.select(&card_selector) {
        if let Some(href) = card.value().attr("href") {
            let endpoint = href.replace("https://blockchair.com/", "");
            if !endpoint.is_empty() {
                // Get block height
                let height = card.select(&block_selector)
                    .next()
                    .and_then(|el| el.value().attr("data-current-value"))
                    .and_then(|h| h.parse::<u64>().ok());

                // Debug: Print all ticker matches for this card
                let tickers: Vec<_> = card.select(&ticker_selector)
                    .map(|el| el.text().collect::<String>().trim().to_string())
                    .collect();
                
                if tickers.len() > 1 {
                    println!("\nMultiple tickers found for {}:", endpoint);
                    println!("Card HTML:\n{}", card.html());
                    println!("All tickers found: {:?}", tickers);
                }

                // Get first ticker symbol (if any)
                let ticker = tickers.into_iter().next();

                println!("Found info for {}: height={:?}, ticker={:?}", 
                    &endpoint, height, ticker);

                blockchain_data.insert(endpoint, BlockchainInfo {
                    height,
                    ticker,
                });
            }
        }
    }
    
    if blockchain_data.is_empty() {
        println!("Warning: No blockchain data found. This might indicate a parsing issue.");
        if let Some(first_card) = document.select(&card_selector).next() {
            println!("First card HTML:\n{}", first_card.html());
        }
    }

    Ok(blockchain_data)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to Redis
    let redis_host = env::var("REDIS_HOST").unwrap_or_else(|_| "redis".into());
    let redis_port = env::var("REDIS_PORT").unwrap_or_else(|_| "6379".into());
    let redis_url = format!("redis://{}:{}", redis_host, redis_port);
    
    let client = redis::Client::open(redis_url.as_str())?;
    let mut conn = client.get_connection()?;
    println!("Connected to Redis at {}", redis_url);

    match get_blockchain_info().await {
        Ok(data) => {
            println!("\nBlockchain information:");
            
            // Create a HashMap to store heights
            let mut heights: HashMap<String, u64> = HashMap::new();
            
            for (chain, info) in &data {
                if let Some(height) = info.height {
                    heights.insert(chain.clone(), height);
                }

                // Keep the console output for debugging
                println!("{}: Height={:?}, Ticker={:?}", 
                    chain, 
                    info.height.map(|h| h.to_string()).unwrap_or_else(|| "N/A".to_string()),
                    info.ticker.as_deref().unwrap_or("N/A")
                );
            }

            // Convert the heights HashMap to JSON and store in Redis
            let json_value = json!(heights);
            match conn.set::<_, _, ()>("http:blockchair.com", json_value.to_string()) {
                Ok(_) => println!("Successfully stored blockchain heights in Redis"),
                Err(e) => eprintln!("Failed to store in Redis: {}", e),
            }

            println!("\nTotal chains tracked: {}", data.len());
        },
        Err(e) => println!("Error: {}", e),
    }
    Ok(())
}
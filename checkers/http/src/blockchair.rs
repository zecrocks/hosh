use scraper::{Html, Selector};
use std::collections::HashMap;

#[derive(Debug)]
pub struct BlockchainInfo {
    pub height: Option<u64>,
    pub ticker: Option<String>,
}

pub async fn get_blockchain_info() -> Result<HashMap<String, BlockchainInfo>, Box<dyn std::error::Error>> {
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
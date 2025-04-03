use scraper::{Html, Selector};
use std::collections::HashMap;
use crate::types::BlockchainInfo;
use std::time::{Duration, Instant};

pub async fn get_regular_blockchain_data(client: &reqwest::Client) -> Result<HashMap<String, BlockchainInfo>, Box<dyn std::error::Error + Send + Sync>> {
    let mut blockchain_data = HashMap::new();
    
    let url = "https://blockchair.com";
    let start_time = Instant::now();
    let response = client.get(url).send().await?;
    let response_time = start_time.elapsed().as_secs_f32() * 1000.0; // Convert to milliseconds
    let text = response.text().await?;
    let document = Html::parse_document(&text);
    
    // Selectors
    let card_selector = Selector::parse("a.blockchain-card").unwrap();
    let block_selector = Selector::parse("div[data-block][data-current-value]").unwrap();
    let ticker_selector = Selector::parse("div.color-text-secondary.flex-shrink-0").unwrap();

    for card in document.select(&card_selector) {
        if let Some(href) = card.value().attr("href") {
            let endpoint = href.replace("https://blockchair.com/", "");
            if !endpoint.is_empty() {
                let height = card.select(&block_selector)
                    .next()
                    .and_then(|el| el.value().attr("data-current-value"))
                    .and_then(|h| h.parse::<u64>().ok());

                let _symbol = card.select(&ticker_selector)
                    .next()
                    .map(|el| el.text().collect::<String>().trim().to_string());

                // Get the logo URL based on endpoint
                let logo_url = match endpoint.as_str() {
                    "bitcoin" => "https://loutre.blockchair.io/w4/assets/images/blockchains/bitcoin/logo_light_48.webp",
                    "bitcoin-cash" => "https://loutre.blockchair.io/w4/assets/images/blockchains/bitcoin-cash/logo_light_48.webp",
                    "ethereum" => "https://loutre.blockchair.io/w4/assets/images/blockchains/ethereum/logo_light_48.webp",
                    "bnb" => "https://loutre.blockchair.io/w4/assets/images/blockchains/bnb/logo_light_48.webp",
                    "cardano" => "https://loutre.blockchair.io/w4/assets/images/blockchains/cardano/logo_light_48.webp",
                    "dogecoin" => "https://loutre.blockchair.io/w4/assets/images/blockchains/dogecoin/logo_light_48.webp",
                    "litecoin" => "https://loutre.blockchair.io/w4/assets/images/blockchains/litecoin/logo_light_48.webp",
                    "polkadot" => "https://loutre.blockchair.io/w4/assets/images/blockchains/polkadot/logo_light_48.webp",
                    "solana" => "https://loutre.blockchair.io/w4/assets/images/blockchains/solana/logo_light_48.webp",
                    "dash" => "https://loutre.blockchair.io/w4/assets/images/blockchains/dash/logo_light_48.webp",
                    "liquid-network" => "https://loutre.blockchair.io/w4/assets/images/blockchains/liquid-network/logo_light_48.webp",
                    _ => "https://blockchair.com/favicon.ico",
                };

                let mut extra = HashMap::new();
                extra.insert("logo_url".to_string(), serde_json::Value::String(logo_url.to_string()));

                blockchain_data.insert(endpoint.clone(), BlockchainInfo {
                    height,
                    name: endpoint.to_string(),
                    response_time_ms: response_time,
                    extra,
                });
            }
        }
    }
    
    if blockchain_data.is_empty() {
        println!("Warning: No blockchain data found from regular site. This might indicate a parsing issue.");
    }

    Ok(blockchain_data)
}

pub async fn get_blockchain_info() -> Result<HashMap<String, BlockchainInfo>, Box<dyn std::error::Error + Send + Sync>> {
    println!("Starting clearnet blockchain info request...");
    
    let regular_client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(Duration::from_secs(30))
        .build()?;

    println!("Created clearnet client, making request...");
    let result = get_regular_blockchain_data(&regular_client).await;
    
    match &result {
        Ok(data) => println!("Clearnet request successful, found {} chains", data.len()),
        Err(e) => println!("Clearnet request failed: {}", e),
    }
    
    result
} 
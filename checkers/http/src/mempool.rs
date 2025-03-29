use scraper::{Html, Selector};
use std::collections::HashMap;
use crate::types::BlockchainInfo;
use std::time::Instant;

#[allow(dead_code)]
pub async fn get_blockchain_info() -> Result<HashMap<String, BlockchainInfo>, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;
    
    let mut blockchain_data = HashMap::new();
    
    // Fetch Bitcoin height
    let btc_url = "https://mempool.space/blocks/1";
    let start_time = Instant::now();
    let btc_response = client.get(btc_url).send().await?;
    let response_time = start_time.elapsed().as_secs_f32() * 1000.0; // Convert to milliseconds
    let btc_html = btc_response.text().await?;
    
    // Print HTML for debugging
    println!("Mempool.space HTML: {}", btc_html);
    
    let btc_document = Html::parse_document(&btc_html);

    // Try to find the height in the blocks table
    let height_selector = Selector::parse("td.height a")
        .expect("Failed to parse height selector");

    if let Some(height_element) = btc_document.select(&height_selector).next() {
        let height_str = height_element.text().collect::<String>();
        println!("Found height text: {}", height_str);
        if let Ok(height) = height_str.parse::<u64>() {
            blockchain_data.insert("bitcoin".to_string(), BlockchainInfo {
                height: Some(height),
                name: "Bitcoin".to_string(),
                response_time_ms: response_time,
                extra: HashMap::new(),
            });
        }
    }

    Ok(blockchain_data)
} 
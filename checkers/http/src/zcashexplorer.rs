use scraper::{Html, Selector};
use std::collections::HashMap;
use crate::types::BlockchainInfo;
use std::time::Instant;

pub async fn get_blockchain_info() -> Result<HashMap<String, BlockchainInfo>, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;
    
    let mut blockchain_data = HashMap::new();
    
    // Fetch Zcash height from mainnet.zcashexplorer.app
    let url = "https://mainnet.zcashexplorer.app/";
    let start_time = Instant::now();
    let response = client.get(url).send().await?;
    let response_time = start_time.elapsed().as_secs_f32() * 1000.0; // Convert to milliseconds
    let html = response.text().await?;
    
    let document = Html::parse_document(&html);

    // Looking for the height in the first row of the recent blocks table
    let height_selector = Selector::parse("td.px-6.py-4.whitespace-nowrap.text-sm.font-medium.text-indigo-600 a")
        .expect("Failed to parse height selector");

    if let Some(height_element) = document.select(&height_selector).next() {
        let height_text = height_element.text().collect::<String>();
        println!("Found height text: {}", height_text);
        
        if let Ok(height) = height_text.trim().parse::<u64>() {
            blockchain_data.insert("zcash".to_string(), BlockchainInfo {
                height: Some(height),
                name: "Zcash".to_string(),
                response_time_ms: response_time,
                extra: HashMap::new(),
            });
        }
    } else {
        println!("Warning: Could not find Zcash height in zcashexplorer HTML");
    }

    Ok(blockchain_data)
} 
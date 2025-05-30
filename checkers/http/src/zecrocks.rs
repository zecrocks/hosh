use scraper::{Html, Selector};
use std::collections::HashMap;
use crate::types::BlockchainInfo;
use std::time::Instant;
use tracing::{info, warn};

pub async fn get_blockchain_info() -> Result<HashMap<String, BlockchainInfo>, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;
    
    let mut blockchain_data = HashMap::new();
    
    // Fetch Zcash height
    let zec_url = "https://explorer.zec.rocks/";
    let start_time = Instant::now();
    let zec_response = client.get(zec_url).send().await?;
    let response_time = start_time.elapsed().as_secs_f32() * 1000.0; // Convert to milliseconds
    let zec_html = zec_response.text().await?;
    
    let zec_document = Html::parse_document(&zec_html);

    // Try to find the height in the first row of the blocks table
    let height_selector = Selector::parse("td.px-6.py-4.whitespace-nowrap.text-sm.font-medium.text-indigo-600 a")
        .expect("Failed to parse height selector");

    if let Some(height_element) = zec_document.select(&height_selector).next() {
        let height_str = height_element.text().collect::<String>().trim().to_string();
        info!("Found Zcash height: {}", height_str);
        if let Ok(height) = height_str.parse::<u64>() {
            blockchain_data.insert("zcash".to_string(), BlockchainInfo {
                height: Some(height),
                name: "Zcash".to_string(),
                response_time_ms: response_time,
                extra: HashMap::new(),
            });
        }
    } else {
        warn!("Could not find Zcash height in HTML");
    }

    Ok(blockchain_data)
} 
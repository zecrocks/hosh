use scraper::{Html, Selector};
use std::collections::HashMap;
use crate::types::BlockchainInfo;
use std::time::Instant;
use tracing::{info, warn, debug};

#[allow(dead_code)]
pub async fn get_blockchain_info() -> Result<HashMap<String, BlockchainInfo>, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;
    
    let mut blockchain_data = HashMap::new();
    
    // Fetch Bitcoin height from the blocks page
    let btc_url = "https://mempool.space/blocks";
    let start_time = Instant::now();
    let btc_response = client.get(btc_url).send().await?;
    let response_time = start_time.elapsed().as_secs_f32() * 1000.0; // Convert to milliseconds
    let btc_html = btc_response.text().await?;
    
    let btc_document = Html::parse_document(&btc_html);

    // Try to find the height in the blocks table
    let height_selector = Selector::parse("table tbody tr td.height a")
        .expect("Failed to parse height selector");

    if let Some(height_element) = btc_document.select(&height_selector).next() {
        let height_str = height_element.text().collect::<String>().trim().to_string();
        info!("Found mempool.space height: {}", height_str);
        if let Ok(height) = height_str.parse::<u64>() {
            blockchain_data.insert("bitcoin".to_string(), BlockchainInfo {
                height: Some(height),
                name: "Bitcoin".to_string(),
                response_time_ms: response_time,
                extra: HashMap::new(),
            });
        } else {
            warn!("Failed to parse mempool.space height as number: {}", height_str);
            warn!("Full HTML content:\n{}", btc_html);
        }
    } else {
        warn!("Could not find mempool.space height in HTML");
        warn!("Full HTML content:\n{}", btc_html);
        // Try alternative selector
        let alt_selector = Selector::parse("td[data-label='Height'] a")
            .expect("Failed to parse alternative height selector");
        if let Some(height_element) = btc_document.select(&alt_selector).next() {
            let height_str = height_element.text().collect::<String>().trim().to_string();
            info!("Found mempool.space height using alternative selector: {}", height_str);
            if let Ok(height) = height_str.parse::<u64>() {
                blockchain_data.insert("bitcoin".to_string(), BlockchainInfo {
                    height: Some(height),
                    name: "Bitcoin".to_string(),
                    response_time_ms: response_time,
                    extra: HashMap::new(),
                });
            } else {
                warn!("Failed to parse mempool.space height as number using alternative selector: {}", height_str);
                warn!("Full HTML content:\n{}", btc_html);
            }
        } else {
            warn!("Could not find mempool.space height using alternative selector");
            warn!("Full HTML content:\n{}", btc_html);
        }
    }

    Ok(blockchain_data)
} 
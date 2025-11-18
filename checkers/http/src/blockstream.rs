use scraper::{Html, Selector};
use std::collections::HashMap;
use crate::types::BlockchainInfo;
use std::time::Instant;

pub async fn get_blockchain_info() -> Result<HashMap<String, BlockchainInfo>, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;
    
    let mut blockchain_data = HashMap::new();
    
    // Fetch Bitcoin height
    let btc_url = "https://blockstream.info/nojs/";
    let start_time = Instant::now();
    let btc_response = client.get(btc_url).send().await?;
    let btc_response_time = start_time.elapsed().as_secs_f32() * 1000.0; // Convert to milliseconds
    let btc_html = btc_response.text().await?;
    
    // Fetch Liquid height
    let liquid_url = "https://blockstream.info/liquid/nojs/";
    let start_time = Instant::now();
    let liquid_response = client.get(liquid_url).send().await?;
    let liquid_response_time = start_time.elapsed().as_secs_f32() * 1000.0; // Convert to milliseconds
    let liquid_html = liquid_response.text().await?;
    
    // Parse documents after all network calls to avoid holding non-Send types across awaits
    let btc_document = Html::parse_document(&btc_html);
    let liquid_document = Html::parse_document(&liquid_html);

    // Selector for both networks (they use the same HTML structure)
    let height_selector = Selector::parse(".blocks-table-cell.highlighted-text[data-label='Height']")
        .expect("Failed to parse height selector");

    // Parse Bitcoin height
    if let Some(height_element) = btc_document.select(&height_selector).next() {
        let height_str = height_element.text().collect::<String>();
        if let Ok(height) = height_str.parse::<u64>() {
            blockchain_data.insert("bitcoin".to_string(), BlockchainInfo {
                height: Some(height),
                name: "Bitcoin".to_string(),
                response_time_ms: btc_response_time,
                extra: HashMap::new(),
            });
        }
    }

    // Parse Liquid height
    if let Some(height_element) = liquid_document.select(&height_selector).next() {
        let height_str = height_element.text().collect::<String>();
        if let Ok(height) = height_str.parse::<u64>() {
            blockchain_data.insert("liquid-network".to_string(), BlockchainInfo {
                height: Some(height),
                name: "Liquid Network".to_string(),
                response_time_ms: liquid_response_time,
                extra: HashMap::new(),
            });
        }
    }

    Ok(blockchain_data)
} 
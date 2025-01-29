use axum::response::Json;
use reqwest::Client as HttpClient;
use std::collections::HashMap;


pub async fn electrum_servers() -> Result<Json<serde_json::Value>, String> {
    let url = "https://raw.githubusercontent.com/spesmilo/electrum/refs/heads/master/electrum/chains/servers.json";

    let http_client = HttpClient::new();
    let response = http_client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch server list: {}", e))?
        .json::<HashMap<String, HashMap<String, serde_json::Value>>>()
        .await
        .map_err(|e| format!("Failed to parse server list JSON: {}", e))?;

    Ok(Json(serde_json::json!({ "servers": response })))
}

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlockchainInfo {
    pub height: Option<u64>,
    pub name: String,  // Human readable name like "Bitcoin", "Ethereum"
    pub response_time_ms: f32,  // Response time in milliseconds
    #[serde(default)]
    pub extra: HashMap<String, Value>,  // Keep Value type for flexibility
    // Removed symbol field since it's not used
} 
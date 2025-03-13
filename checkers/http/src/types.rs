use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlockchainInfo {
    pub height: Option<u64>,
    pub name: String,  // Human readable name like "Bitcoin", "Ethereum"
    #[serde(default)]
    pub extra: HashMap<String, Value>,  // Keep Value type for flexibility
    // Removed symbol field since it's not used
} 
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlockchainInfo {
    pub height: Option<u64>,
    pub name: String,
    pub symbol: String,
    #[serde(default)]
    pub extra: HashMap<String, Value>,
} 
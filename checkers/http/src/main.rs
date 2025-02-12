use redis::Commands;
use std::env;
use std::collections::HashMap;
use serde_json::json;
use std::error::Error;
use std::fmt;

mod blockchair;
mod blockchain;

#[derive(Debug)]
enum CheckerError {
    Redis(redis::RedisError),
    Reqwest(reqwest::Error),
    Var(std::env::VarError),
    Other(String),
}

impl fmt::Display for CheckerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CheckerError::Redis(e) => write!(f, "Redis error: {}", e),
            CheckerError::Reqwest(e) => write!(f, "Request error: {}", e),
            CheckerError::Var(e) => write!(f, "Environment variable error: {}", e),
            CheckerError::Other(e) => write!(f, "Error: {}", e),
        }
    }
}

impl Error for CheckerError {}

impl From<redis::RedisError> for CheckerError {
    fn from(err: redis::RedisError) -> CheckerError {
        CheckerError::Redis(err)
    }
}

impl From<reqwest::Error> for CheckerError {
    fn from(err: reqwest::Error) -> CheckerError {
        CheckerError::Reqwest(err)
    }
}

impl From<std::env::VarError> for CheckerError {
    fn from(err: std::env::VarError) -> CheckerError {
        CheckerError::Var(err)
    }
}

#[tokio::main]
async fn main() -> Result<(), CheckerError> {
    // Get Redis connection details from environment variables
    let redis_host = env::var("REDIS_HOST").unwrap_or_else(|_| "redis".to_string());
    let redis_port = env::var("REDIS_PORT").unwrap_or_else(|_| "6379".to_string());
    let redis_url = format!("redis://{}:{}", redis_host, redis_port);
    
    println!("Connecting to Redis at {}", redis_url);

    // Connect to Redis with retry logic
    let client = redis::Client::open(redis_url.as_str())?;
    let mut con = match client.get_connection() {
        Ok(con) => con,
        Err(e) => {
            eprintln!("Failed to connect to Redis: {}", e);
            eprintln!("Make sure Redis is running and accessible at {}", redis_url);
            return Err(e.into());
        }
    };

    let mut explorer_data = HashMap::new();

    // Fetch blockchain heights from blockchain.com
    match blockchain::get_blockchain_info().await {
        Ok(blockchain_heights) => {
            println!("\nBlockchain.com Block Heights:");
            let mut heights = HashMap::new();
            for (symbol, info) in &blockchain_heights {
                if let Some(height) = info.height {
                    println!("{}: Height=\"{}\"", symbol, height);
                    heights.insert(symbol.to_string(), height);
                }
            }
            explorer_data.insert("https://www.blockchain.com/explorer", heights);
            println!("\nTotal blockchain.com heights tracked: {}", blockchain_heights.len());
        },
        Err(e) => println!("Error fetching blockchain.com heights: {}", e),
    }

    // Fetch blockchain heights from blockchair
    match blockchair::get_blockchain_info().await {
        Ok(blockchair_heights) => {
            println!("\nBlockchair Block Heights:");
            let mut heights = HashMap::new();
            for (chain, info) in &blockchair_heights {
                if let Some(height) = info.height {
                    let display_name = info.ticker.as_ref().unwrap_or(chain);
                    println!("{}: Height=\"{}\"", display_name, height);
                    heights.insert(chain.to_string(), height);
                }
            }
            explorer_data.insert("https://blockchair.com", heights);
            println!("\nTotal blockchair heights tracked: {}", blockchair_heights.len());
        },
        Err(e) => println!("Error fetching blockchair heights: {}", e),
    }

    // Store all heights in Redis as a single JSON value
    let json_value = json!(explorer_data);
    let _: () = con.set("http:heights", json_value.to_string())?;

    println!("\nTotal explorers tracked: {}", explorer_data.len());
    println!("Successfully stored blockchain heights in Redis");

    Ok(())
}
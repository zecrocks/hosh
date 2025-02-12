use redis::Commands;
use std::error::Error;
use std::fmt;
use std::time::Duration;
use tokio::time::sleep;

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
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let redis_host = std::env::var("REDIS_HOST").unwrap_or_else(|_| {
        println!("⚠️  REDIS_HOST not set, using default 'redis'");
        "redis".to_string()
    });
    let redis_port = std::env::var("REDIS_PORT").unwrap_or_else(|_| {
        println!("⚠️  REDIS_PORT not set, using default '6379'");
        "6379".to_string()
    });
    let redis_url = format!("redis://{redis_host}:{redis_port}");
    
    println!("🔌 Connecting to Redis at {}", redis_url);
    let client = redis::Client::open(redis_url.as_str())?;
    let mut con = client.get_connection()?;

    loop {
        // Get blockchain.com heights
        match blockchain::get_blockchain_info().await {
            Ok(heights) => {
                println!("\nBlockchain.com Block Heights:");
                for (symbol, info) in &heights {
                    if let Some(height) = info.height {
                        println!("{}: Height=\"{}\"", symbol, height);
                        let _: () = con.set(
                            format!("http:blockchain.{}", symbol),
                            height
                        )?;
                    }
                }
                println!("\nTotal blockchain.com heights tracked: {}", heights.len());
            }
            Err(e) => println!("Error fetching blockchain.com heights: {}", e),
        }

        // Get blockchair heights
        match blockchair::get_blockchain_info().await {
            Ok(heights) => {
                println!("\nBlockchair Block Heights:");
                for (symbol, info) in &heights {
                    if let Some(height) = info.height {
                        println!("{}: Height=\"{}\"", symbol, height);
                        let _: () = con.set(
                            format!("http:blockchair.{}", symbol),
                            height
                        )?;
                    }
                }
                println!("\nTotal blockchair heights tracked: {}", heights.len());
            }
            Err(e) => println!("Error fetching blockchair heights: {}", e),
        }

        // Sleep for 5 minutes
        println!("\nSleeping for 5 minutes...");
        sleep(Duration::from_secs(300)).await;
    }
}
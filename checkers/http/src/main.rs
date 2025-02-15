use redis::Commands;
use std::error::Error;
use std::fmt;
use std::time::Duration;
use tokio::time::sleep;
use std::collections::HashMap;

mod blockchair;
mod blockchain;
mod blockstream;
mod mempool;
mod zecrocks;
mod zcashexplorer;

// Keep this import since we'll use it as our canonical BlockchainInfo
use blockchain::BlockchainInfo;

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
async fn main() -> Result<(), Box<dyn Error>> {
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
        // Fetch data from all sources concurrently
        let (blockstream_result, /*mempool_result,*/ zecrocks_result, blockchair_result, blockchain_result, zcashexplorer_result) = tokio::join!(
            blockstream::get_blockchain_info(),
            // mempool::get_blockchain_info(),  // Commented out until we can parse it properly
            zecrocks::get_blockchain_info(),
            blockchair::get_blockchain_info(),
            blockchain::get_blockchain_info(),
            zcashexplorer::get_blockchain_info()
        );

        // Add blockstream data
        if let Ok(data) = blockstream_result {
            for (chain, info) in data {
                if let Some(height) = info.height {
                    println!("{} height: {} (blockstream)", info.name, height);
                    let _: () = con.set(
                        format!("http:blockstream.{}", chain),
                        height
                    )?;
                }
            }
        }

        // Add zecrocks data
        if let Ok(data) = zecrocks_result {
            for (chain, info) in data {
                if let Some(height) = info.height {
                    println!("{} height: {} (zecrocks)", info.name, height);
                    let _: () = con.set(
                        format!("http:zecrocks.{}", chain),
                        height
                    )?;
                }
            }
        }

        // Add blockchair data
        if let Ok(data) = blockchair_result {
            for (chain, info) in data {
                if let Some(height) = info.height {
                    println!("{} height: {} (blockchair)", info.name, height);
                    let _: () = con.set(
                        format!("http:blockchair.{}", chain),
                        height
                    )?;
                }
            }
        }

        // Add blockchain.com data
        if let Ok(data) = blockchain_result {
            for (chain, info) in data {
                if let Some(height) = info.height {
                    println!("{} height: {} (blockchain)", info.name, height);
                    let _: () = con.set(
                        format!("http:blockchain.{}", chain),
                        height
                    )?;
                }
            }
        }

        // Add zcashexplorer data
        if let Ok(data) = zcashexplorer_result {
            for (chain, info) in data {
                if let Some(height) = info.height {
                    println!("{} height: {} (zcashexplorer)", info.name, height);
                    let _: () = con.set(
                        format!("http:zcashexplorer.{}", chain),
                        height
                    )?;
                }
            }
        }

        // Sleep for 5 minutes
        println!("\nSleeping for 5 minutes...");
        sleep(Duration::from_secs(300)).await;
    }
}
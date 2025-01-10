use electrum_client::{Client, ElectrumApi};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to the Electrum server
    let client = Client::new("ssl://electrum.blockstream.info:50002")?;
    
    // Fetch the latest block height
    let block_height = client.block_headers_subscribe()?.height;
    println!("Latest block height: {}", block_height);
    
    // Fetch the block header for the latest block
    let block_header = client.block_header(block_height)?;
    println!("Latest block header: {:?}", block_header);

    Ok(())
}


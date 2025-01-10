use std::error::Error;
use http::Uri;
use rustls::crypto::ring::default_provider;
use zingolib;

fn main() -> Result<(), Box<dyn Error>> {
    default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let uri: Uri = "https://zec.rocks:443"
        .parse()
        .expect("Failed to parse URI");
    let block_height = zingolib::get_latest_block_height(uri)?;
    println!("Latest block height: {}", block_height);

    Ok(())
}

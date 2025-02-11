use std::error::Error;
use http::Uri;
use rustls::crypto::ring::default_provider;
use zingolib::grpc_connector;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Install the crypto provider properly
    default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let uri: Uri = "https://zec.rocks:443"
        .parse()
        .expect("Failed to parse URI");

    // Get server info directly using grpc_connector
    match grpc_connector::get_info(uri.clone()).await {
        Ok(info) => {
            println!("{:<25} {}", "Host:", uri);
            println!("{:<25} {}", "Block height:", info.block_height);
            println!("{:<25} {}", "Vendor:", info.vendor);
            println!("{:<25} {}", "Git commit:", info.git_commit);
            println!("{:<25} {}", "Chain name:", info.chain_name);
            println!("{:<25} {}", "Sapling activation:", info.sapling_activation_height);
            println!("{:<25} {}", "Consensus branch ID:", info.consensus_branch_id);
            println!("{:<25} {}", "Taddr support:", info.taddr_support);
            println!("{:<25} {}", "Branch:", info.branch);
            println!("{:<25} {}", "Build date:", info.build_date);
            println!("{:<25} {}", "Build user:", info.build_user);
            println!("{:<25} {}", "Estimated height:", info.estimated_height);
            println!("{:<25} {}", "LWD Version:", info.version);
            println!("{:<25} {}", "Zcashd build/version:", info.zcashd_build);
            println!("{:<25} {}", "Zcashd subversion:", info.zcashd_subversion);
        }
        Err(e) => println!("Error getting server info: {}", e),
    }

    Ok(())
} 
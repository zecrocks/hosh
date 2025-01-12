use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_native_tls::TlsConnector;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let host = "electrum.blockstream.info";
    let port = 50002;

    // Address and connection setup
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(addr).await?;
    println!("Connected to {}", host);

    // Wrap the TCP connection with TLS
    let tls_connector = TlsConnector::from(native_tls::TlsConnector::new()?);
    let mut stream = tls_connector.connect(host, stream).await?;
    println!("TLS connection established");

    // Send the JSON-RPC request
    let request = json!({
        "id": 1,
        "method": "server.peers.subscribe",
        "params": []
    });
    let request_str = serde_json::to_string(&request)? + "\n";
    stream.write_all(request_str.as_bytes()).await?;
    println!("Request sent: {}", request_str);

    // Read the response until it's complete
    let mut response_str = String::new();
    loop {
        let mut buffer = vec![0; 4096];
        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            break; // EOF reached
        }
        response_str.push_str(&String::from_utf8_lossy(&buffer[..n]));
        if response_str.ends_with("\n") {
            break; // Assuming JSON-RPC responses end with a newline
        }
    }
    println!("Response received: {}", response_str);

    // Parse and display the response
    match serde_json::from_str::<serde_json::Value>(&response_str) {
        Ok(response) => {
            if let Some(peers) = response["result"].as_array() {
                println!("Discovered peers:");
                for peer in peers {
                    if let Some(peer_details) = peer.as_array() {
                        let address = peer_details.get(0).and_then(|v| v.as_str()).unwrap_or("Unknown");
                        let hostname = peer_details.get(1).and_then(|v| v.as_str()).unwrap_or("Unknown");

                        // Use a longer-lived default empty vector
                        let empty_features = vec![];
                        let features = peer_details.get(2).and_then(|v| v.as_array()).unwrap_or(&empty_features);

                        let version = features.iter().find_map(|f| f.as_str().filter(|s| s.starts_with('v'))).unwrap_or("Unknown");
                        let ports: Vec<&str> = features.iter().filter_map(|f| f.as_str()).collect();

                        println!(
                            "- Address: {}\n  Hostname: {}\n  Version: {}\n  Features: {:?}",
                            address, hostname, version, ports
                        );
                    }
                }
            } else {
                println!("No peers discovered.");
            }
        }
        Err(e) => {
            println!("Failed to parse JSON: {}", e);
            println!("Raw response: {}", response_str);
        }
    }

    Ok(())
}


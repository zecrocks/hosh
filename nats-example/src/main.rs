use std::time::Duration;
use std::thread;
use nats;
use ctrlc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting NATS example...");
    
    // Graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        println!("\nShutting down...");
        r.store(false, Ordering::Relaxed);
        // Force exit immediately if Ctrl+C is pressed twice
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");
    
    // Configs
    let nats_addr = env::var("NATS_ADDR").unwrap_or_else(|_| "nats:4222".to_string());
    let nats_url = format!("nats://{}", nats_addr);
    let subject = env::var("NATS_SUBJECT").unwrap_or_else(|_| "echo".to_string());
    
    // Connections
    let nc = nats::connect(&nats_url)?;
    let nc_sub = nats::connect(&nats_url)?;
    let sub = nc_sub.subscribe(&subject)?;

    // Spawn a thread to listen for incoming messages
    {
        let running = running.clone();
        std::thread::spawn(move || {
            while running.load(Ordering::Relaxed) {
                if let Some(msg) = sub.try_next() {
                    println!("Received: {}", String::from_utf8_lossy(&msg.data));
                }
                thread::sleep(Duration::from_millis(100));
            }
            println!("Subscriber thread shutting down");
        })
    };

    // Publish messages until shutdown requested
    while running.load(Ordering::Relaxed) {
        match nc.publish(&subject, "Hello World!") {
            Ok(_) => println!("Published: Hello World!"),
            Err(e) => {
                eprintln!("Failed to publish: {}", e);
                break;
            }
        }
        thread::sleep(Duration::from_secs(5));
    }

    // Give threads a chance to clean up
    thread::sleep(Duration::from_secs(1));
    println!("Shutdown complete");
    Ok(())
}


use std::{env, error::Error, time::Duration};
use serde::{Deserialize, Serialize};
use tokio::time;
use chrono::{DateTime, Utc};
use tracing::{info, error};
use reqwest::Client;
use tracing_subscriber;
use scraper::{Html, Selector};

// Environment variable constants
const DEFAULT_DISCOVERY_INTERVAL: u64 = 3600; // 1 hour default

// ClickHouse configuration
struct ClickHouseConfig {
    url: String,
    user: String,
    password: String,
    database: String,
    client: reqwest::Client,
}

impl ClickHouseConfig {
    fn from_env() -> Self {
        let host = env::var("CLICKHOUSE_HOST").unwrap_or_else(|_| "chronicler".into());
        let port = env::var("CLICKHOUSE_PORT").unwrap_or_else(|_| "8123".into());
        let url = format!("http://{}:{}", host, port);
        info!("Configuring ClickHouse connection to {}", url);
        
        Self {
            url,
            user: env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "hosh".into()),
            password: env::var("CLICKHOUSE_PASSWORD").expect("CLICKHOUSE_PASSWORD environment variable must be set"),
            database: env::var("CLICKHOUSE_DB").unwrap_or_else(|_| "hosh".into()),
            client: reqwest::Client::new(),
        }
    }

    async fn execute_query(&self, query: &str) -> Result<String, Box<dyn Error>> {
        info!("Executing ClickHouse query");
        let response = self.client.post(&self.url)
            .basic_auth(&self.user, Some(&self.password))
            .header("Content-Type", "text/plain")
            .body(query.to_string())
            .send()
            .await?;
        
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await?;
            error!("ClickHouse query failed with status {}: {}", status, error_text);
            return Err(format!("ClickHouse query failed: {}", error_text).into());
        }
        
        let result = response.text().await?;
        info!("ClickHouse query executed successfully");
        Ok(result)
    }

    async fn target_exists(&self, module: &str, hostname: &str, port: u16) -> Result<bool, Box<dyn Error>> {
        let query = format!(
            "SELECT count() FROM {}.targets WHERE module = '{}' AND hostname = '{}' AND port = {}",
            self.database, module, hostname, port
        );
        let result = self.execute_query(&query).await?;
        Ok(result.trim().parse::<i64>()? > 0)
    }

    async fn insert_target(&self, module: &str, hostname: &str, port: u16, community: bool) -> Result<(), Box<dyn Error>> {
        if self.target_exists(module, hostname, port).await? {
            info!("Target already exists: {} {}:{}", module, hostname, port);
            return Ok(());
        }

        let query = format!(
            "INSERT INTO TABLE {}.targets (target_id, module, hostname, port, last_queued_at, last_checked_at, user_submitted, community) VALUES (generateUUIDv4(), '{}', '{}', {}, now64(3, 'UTC'), now64(3, 'UTC'), false, {})",
            self.database, module, hostname, port, community
        );
        self.execute_query(&query).await?;
        info!("Successfully inserted target: {} {}:{} (community: {})", module, hostname, port, community);
        Ok(())
    }
}

// Static ZEC server configuration
// (hostname, port, is_community)
const ZEC_SERVERS: &[(&str, u16, bool)] = &[
    // Official zec.rocks servers
    ("zec.rocks", 443, false),
    ("ap.zec.rocks", 443, false),
    ("eu.zec.rocks", 443, false),
    ("na.zec.rocks", 443, false),
    ("sa.zec.rocks", 443, false),
    ("zcashd.zec.rocks", 443, false),
    ("zaino.unsafe.zec.rocks", 443, false),
    ("zaino.testnet.unsafe.zec.rocks", 443, false),
    ("zcash.mysideoftheweb.com", 9067, false), // eZcash
    // Ywallet nodes
    ("lwd1.zcash-infra.com", 9067, false),
    ("lwd2.zcash-infra.com", 9067, false),
    ("lwd3.zcash-infra.com", 9067, false),
    ("lwd4.zcash-infra.com", 9067, false),
    ("lwd5.zcash-infra.com", 9067, false),
    ("lwd6.zcash-infra.com", 9067, false),
    ("lwd7.zcash-infra.com", 9067, false),
    ("lwd8.zcash-infra.com", 9067, false),
    //// Testnet servers
    ("testnet.zec.rocks", 443, false),
    ("lightwalletd.testnet.electriccoin.co", 9067, false),
    ("zcash.mysideoftheweb.com", 19067, false), // eZcash (Testnet)
    //// Tor nodes
    // Zec.rocks Mainnet (Zebra + Zaino)
    // ("6fiyttjv3awhv6afdqeeerfxckdqlt6vejjsadeiqawnt7e3hxdcaxqd.onion", 443, false),
    // ("lzzfytqg24a7v6ejqh2q4ecaop6mf62gupvdimc4ryxeixtdtzxxjmad.onion", 443, false),
    // ("vzzwzsmru5ybxkfqxefojbmkh5gefzeixvquyonleujiemhr3dypzoid.onion", 443, false),
    // Zec.rocks Mainnet (Zcashd + Lightwalletd)
    // ("ltefw7pqlslcst5n465kxwgqmb4wxwp7djvhzqlfwhh3wx53xzuwr2ad.onion", 443, false),
    // Zec.rocks Testnet (Zebra + Zaino)
    // ("gnsujqzqaepdmxjq4ixm74kapd7grp3j5selm7nsejz6ctxa3yx4q3yd.onion", 443, false),
    // ("ti64zsaj6w66um42o4nyjtstzg4zryqkph2c45x4bwfqhydxeznrfgad.onion", 443, false),
    //// Community nodes
    // ("zaino.stakehold.rs", 443, true), // disabled for now because it's not responding
    // ("lightwalletd.stakehold.rs", 443, true), // disabled for now because it's not responding
    ("zeclwnode.mylabtest.vip", 9067, true),
    ("z.arounder.co", 9067, true),
    ("z.arounder.co", 443, true),
    ("zec.alexxiy.top", 9067, true),
    ("zec.alexxiy.top", 8137, true),
    ("z.dptr.capital", 9067, true),
    ("z.miscthings.casa", 9067, true),
    ("z.miscthings.casa", 443, true),
    ("zlw.nodemaster.link", 9067, true),
    ("light.myown.party", 443, true),
    ("znode.roamerx.win", 443, true),
    ("zec.leoninedao.org", 8232, true),
    ("zec.leoninedao.org", 8137, true),
    ("zec.bitchat.channel", 8443, true),
    ("zaino.netstable.stream", 443, true),
    ("chmodas.org", 443, true),
    ("lightwallet.netstable.stream", 9067, true),
    ("lwal.podev.name", 443, true),
    ("lwal.podev.name", 9067, true),
    ("zcash.johndo.men", 443, true),
    ("zwallet.techly.fyi", 443, true),
    ("zecapi.linkpc.net", 443, true),
    ("zw.run.place", 443, true),
    ("lightwallet.qzz.io", 443, true),
    ("zecrpc.dpdns.org", 443, true),
    ("carover0.xyz", 9067, true),
    ("lwd.z0n.jp", 443, true),
    ("ltwallet.netter.top", 9067, true),
    ("lzec.linked.sarl", 443, true),
    ("stroopwafel.observer", 9067, true),
    ("zcw.sererra.link", 9067, true),
    ("lightzec.dinks.pw", 443, true),
    ("zzz.stripest.online", 443, true),
    ("lightapi.justneedto.click", 9067, true),
    ("zecnode.sarl", 443, true),
    ("zcash-chainmatter.tailb19703.ts.net", 443, true),
];

// Static HTTP block explorer configuration
const HTTP_EXPLORERS: &[(&str, &str)] = &[
    ("blockchair", "https://blockchair.com"),
    ("blockstream", "https://blockstream.info"),
    ("zecrocks", "https://explorer.zec.rocks"),
    ("blockchain", "https://blockchain.com"),
    ("zcashexplorer", "https://mainnet.zcashexplorer.app"),
];

#[derive(Debug, Deserialize)]
struct BtcServerDetails {
    #[serde(default)]
    s: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ServerData {
    host: String,
    port: u16,
    #[serde(default)]
    height: u64,
    #[serde(default)]
    status: String,
    error: Option<String>,
    last_updated: DateTime<Utc>,
    #[serde(default)]
    ping: f64,
    #[serde(default)]
    version: Option<String>,
}

async fn fetch_btc_servers() -> Result<std::collections::HashMap<String, BtcServerDetails>, Box<dyn Error>> {
    info!("Fetching BTC servers from Electrum repository...");
    let client = reqwest::Client::new();
    let response = client
        .get("https://raw.githubusercontent.com/spesmilo/electrum/refs/heads/master/electrum/chains/mainnet/servers.json")
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    
    let servers: std::collections::HashMap<String, BtcServerDetails> = response.json().await?;
    info!("Found {} BTC servers", servers.len());
    Ok(servers)
}

async fn get_server_details(client: &Client, host: &str, port: u16) -> Result<ServerData, Box<dyn Error>> {
    let start_time = std::time::Instant::now();
    let url = format!("http://{}:{}", host, port);
    
    let response = client.get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await?;
    
    let ping = start_time.elapsed().as_secs_f64();
    let version = response.headers()
        .get("server")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    
    Ok(ServerData {
        host: host.to_string(),
        port,
        height: 0, // We'll get this from the server response in the future
        status: "active".to_string(),
        error: None,
        last_updated: Utc::now(),
        ping,
        version,
    })
}

async fn get_blockchair_onion_url(client: &Client) -> Result<Option<String>, Box<dyn Error>> {
    let url = "https://blockchair.com";
    let response = client.get(url).send().await?;
    let text = response.text().await?;
    let document = Html::parse_document(&text);
    
    // Use a more specific selector to target the onion URL link directly
    let link_selector = Selector::parse("a[href*='.onion']").unwrap();
    
    if let Some(link) = document.select(&link_selector).next() {
        if let Some(href) = link.value().attr("href") {
            // Only return the URL if it contains the blkchair prefix
            if href.contains("blkchair") {
                info!("Found Blockchair onion URL: {}", href);
                return Ok(Some(href.to_string()));
            } else {
                info!("Found onion URL but it's not Blockchair's: {}", href);
            }
        }
    }
    
    info!("No Blockchair onion URL found");
    Ok(None)
}

async fn update_servers(
    client: &reqwest::Client,
    clickhouse: &ClickHouseConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    // Process ZEC servers first
    info!("Processing {} ZEC servers...", ZEC_SERVERS.len());
    for (host, port, community) in ZEC_SERVERS {
        info!("Processing ZEC server: {}:{} (community: {})", host, port, community);
        if !clickhouse.target_exists("zec", host, *port).await? {
            if let Err(e) = clickhouse.insert_target("zec", host, *port, *community).await {
                error!("Failed to insert ZEC server {}:{}: {}", host, port, e);
            }
        } else {
            info!("ZEC server {}:{} already exists, skipping", host, port);
        }
    }

    // Process HTTP block explorers second
    info!("Processing {} HTTP block explorers...", HTTP_EXPLORERS.len());
    for (explorer, url) in HTTP_EXPLORERS {
        info!("Processing HTTP explorer: {} ({})", explorer, url);
        
        // Insert the main explorer target if it doesn't exist
        if !clickhouse.target_exists("http", url, 80).await? {
            if let Err(e) = clickhouse.insert_target("http", url, 80, false).await {
                error!("Failed to insert HTTP explorer {}: {}", url, e);
                continue;
            }
        } else {
            info!("HTTP explorer {} already exists, skipping", url);
        }

        // Special handling for Blockchair to get onion URL
        if explorer == &"blockchair" {
            if let Some(onion_url) = get_blockchair_onion_url(client).await? {
                info!("Found Blockchair onion URL: {}", onion_url);
                if !clickhouse.target_exists("http", &onion_url, 80).await? {
                    if let Err(e) = clickhouse.insert_target("http", &onion_url, 80, false).await {
                        error!("Failed to insert Blockchair onion URL {}: {}", onion_url, e);
                    }
                } else {
                    info!("Blockchair onion URL {} already exists, skipping", onion_url);
                }
            } else {
                error!("Failed to get Blockchair onion URL");
            }
        }
    }

    // Process BTC servers last
    let btc_servers = fetch_btc_servers().await?;
    info!("Processing {} BTC servers...", btc_servers.len());
    for (host, details) in btc_servers {
        let port = details.s
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(50001);
        info!("Processing BTC server: {}:{}", host, port);
        
        if !clickhouse.target_exists("btc", &host, port).await? {
            // Try to get details but don't require success
            let details = get_server_details(client, &host, port).await;
            match details {
                Ok(_) => {
                    if let Err(e) = clickhouse.insert_target("btc", &host, port, false).await {
                        error!("Failed to insert BTC server {}:{}: {}", host, port, e);
                    }
                }
                Err(e) => {
                    // Still insert the target even if verification fails
                    info!("Could not verify BTC server {}:{}: {}, but inserting anyway", host, port, e);
                    if let Err(e) = clickhouse.insert_target("btc", &host, port, false).await {
                        error!("Failed to insert BTC server {}:{}: {}", host, port, e);
                    }
                }
            }
        } else {
            info!("BTC server {}:{} already exists, skipping", host, port);
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing subscriber with environment filter
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env());
    subscriber.init();

    info!("Starting discovery service...");

    // Initialize ClickHouse client
    let clickhouse = ClickHouseConfig::from_env();
    let http_client = Client::new();
    info!("Initialized ClickHouse client");

    // Get discovery interval from environment or use default
    let discovery_interval = env::var("DISCOVERY_INTERVAL")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_DISCOVERY_INTERVAL);

    info!("Discovery interval set to {} seconds", discovery_interval);

    loop {
        info!("Starting discovery cycle...");
        
        match update_servers(&http_client, &clickhouse).await {
            Ok(_) => info!("Discovery cycle completed successfully"),
            Err(e) => error!("Error during discovery cycle: {}", e),
        }

        info!("Sleeping for {} seconds before next discovery cycle", discovery_interval);
        time::sleep(Duration::from_secs(discovery_interval)).await;
    }
} 

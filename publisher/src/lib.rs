pub mod config;
pub mod models;
pub mod publisher;
pub mod redis_store;

pub use models::ServerData;
pub use publisher::Publisher;
pub use config::Config;

// Remove the duplicate Config struct and its implementation
// Delete everything between here...
// pub struct Config {
//     pub refresh_interval: u64,
//     pub chain_intervals: HashMap<String, u64>,
//     pub nats_url: String,
//     pub nats_prefix: String,
//     pub redis_host: String,
//     pub redis_port: u16,
//     pub nats_username: String,
//     pub nats_password: String,
// }

// impl Config {
//     pub fn from_env() -> Result<Self> {
//         ...
//     }
// }
// ... and here 
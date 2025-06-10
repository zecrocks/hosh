use crate::config::{Config, PREFIXES};
use crate::clickhouse::ClickHouseClient;
use anyhow::Result;
use async_nats::Client as NatsClient;
use tracing::{debug, error, info};
use tokio::time::{interval_at, Instant, Duration};

pub struct Publisher {
    nats: NatsClient,
    clickhouse: ClickHouseClient,
    config: Config,
}

impl Publisher {
    pub fn new(nats: NatsClient, config: Config) -> Self {
        let clickhouse = ClickHouseClient::new(
            config.clickhouse_url.clone(),
            config.clickhouse_db.clone(),
            config.clickhouse_user.clone(),
            config.clickhouse_password.clone(),
        );
        
        Self {
            nats,
            clickhouse,
            config,
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!(
            interval = self.config.check_interval,
            "🕒 Starting publisher service - will check targets every {} seconds", 
            self.config.check_interval
        );

        // Run first check immediately
        self.run_check_cycle().await?;

        // Create interval starting after the first check_interval
        let start = Instant::now() + Duration::from_secs(self.config.check_interval);
        let mut interval = interval_at(start, Duration::from_secs(self.config.check_interval));

        loop {
            interval.tick().await;
            self.run_check_cycle().await?;
        }
    }

    async fn run_check_cycle(&self) -> Result<()> {
        info!(
            "⏰ Starting check cycle for all networks (interval: {} seconds)",
            self.config.check_interval
        );
        
        let mut handles = Vec::new();
        
        for &prefix in PREFIXES {
            let handle = tokio::spawn(Self::publish_checks_for_chain(
                self.nats.clone(),
                self.clickhouse.clone(),
                self.config.clone(),
                prefix,
            ));
            handles.push(handle);
        }

        match futures::future::try_join_all(handles).await {
            Ok(_) => info!("✅ Completed check cycle for all networks"),
            Err(e) => error!(%e, "❌ Publisher task failed"),
        }

        Ok(())
    }

    pub async fn publish_checks_for_chain(
        nats: NatsClient,
        clickhouse: ClickHouseClient,
        config: Config,
        prefix: &'static str,
    ) -> Result<()> {
        let module = prefix.trim_end_matches(':');
        
        let targets = clickhouse.get_targets_for_module(module).await?;
        let target_count = targets.len();
        
        if target_count == 0 {
            info!(module, "No targets found for module - skipping publish cycle");
            return Ok(());
        }
        
        let mut published = 0;
        
        if module == "http" {
            // Track unique explorer URLs to avoid duplicates
            let mut published_urls = std::collections::HashSet::new();

            for target in targets {
                if let Err(e) = clickhouse.update_last_queued(&target.target_id).await {
                    error!(%e, "Failed to update last_queued_at");
                    continue;
                }

                // Use the hostname directly as the URL
                let url = target.hostname;

                // Skip if we've already published a check for this URL
                if !published_urls.insert(url.clone()) {
                    debug!("Skipping duplicate explorer URL: {}", url);
                    continue;
                }

                let subject = format!("{}check.{}", config.nats_prefix, module);
                let payload = serde_json::json!({
                    "url": url,
                    "port": 80,
                    "check_id": target.target_id,
                    "user_submitted": false,
                    "dry_run": false
                });
                
                if let Err(e) = nats.publish(subject, payload.to_string().into()).await {
                    error!(%e, "Failed to publish check request");
                    continue;
                }

                published += 1;
                info!(
                    url = %url,
                    module,
                    "Published check request for explorer", 
                );
            }
            
            info!(
                module,
                total = published,
                "✅ Published checks for {} unique explorers",
                published
            );
        } else {
            // For other modules, process each target individually
            for target in targets {
                if let Err(e) = clickhouse.update_last_queued(&target.target_id).await {
                    error!(%e, "Failed to update last_queued_at");
                    continue;
                }

                let subject = format!("{}check.{}", config.nats_prefix, module);
                
                // Format the message payload based on the module type
                let payload = match module {
                    "zec" => serde_json::json!({
                        "host": target.hostname,
                        "port": target.port,  // Use target's port directly
                        "check_id": target.target_id,
                        "user_submitted": false
                    }),
                    "btc" => serde_json::json!({
                        "host": target.hostname,
                        "port": target.port,  // Use target's port directly
                        "check_id": target.target_id,
                        "user_submitted": false
                    }),
                    _ => serde_json::to_value(&target)?
                };
                
                if let Err(e) = nats.publish(subject, payload.to_string().into()).await {
                    error!(%e, "Failed to publish check request");
                    continue;
                }

                published += 1;
                if published % 10 == 0 || published == target_count {
                    info!(
                        target = %target.hostname,
                        module,
                        "Published check request ({}/{})", 
                        published, 
                        target_count
                    );
                }
            }

            info!(
                module,
                total = target_count,
                published = published,
                "✅ Published {}/{} checks",
                published,
                target_count
            );
        }

        Ok(())
    }
} 
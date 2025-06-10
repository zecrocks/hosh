use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{error, info, trace};
use reqwest::Client;
use chrono::{DateTime, Utc, NaiveDateTime};

#[derive(Debug, Clone)]
pub struct ClickHouseClient {
    http_client: Client,
    url: String,
    database: String,
    user: String,
    password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Target {
    pub target_id: String,
    pub module: String,
    pub hostname: String,
    pub port: u16,
    pub last_queued_at: DateTime<Utc>,
    pub last_checked_at: DateTime<Utc>,
    pub user_submitted: bool,
}

impl ClickHouseClient {
    pub fn new(url: String, database: String, user: String, password: String) -> Self {
        Self {
            http_client: Client::new(),
            url,
            database,
            user,
            password,
        }
    }

    pub async fn get_targets_for_module(&self, module: &str) -> Result<Vec<Target>> {
        let query = format!(
            "SELECT target_id, hostname, module, port, last_queued_at, last_checked_at, user_submitted 
             FROM {}.targets 
             WHERE module = '{}'
             ORDER BY last_checked_at ASC
             FORMAT TabSeparatedWithNames",
            self.database, module
        );

        trace!(
            %module,
            %query,
            "Executing ClickHouse query"
        );

        let response = self.http_client
            .post(&self.url)
            .query(&[
                ("database", &self.database),
                ("user", &self.user),
                ("password", &self.password),
            ])
            .header("Content-Type", "text/plain")
            .body(query)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            error!(
                %module,
                %status,
                %body,
                "ClickHouse query failed"
            );
            return Err(anyhow::anyhow!("ClickHouse query failed: {}", body));
        }

        let mut targets = Vec::new();
        let mut lines = body.lines();
        lines.next(); // Skip header
        
        for line in lines {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() != 7 {
                error!(
                    %module,
                    %line,
                    "Invalid number of fields in TSV response"
                );
                continue;
            }

            match (|| -> Result<Target> {
                let parse_datetime = |dt_str: &str| -> Result<DateTime<Utc>> {
                    NaiveDateTime::parse_from_str(dt_str, "%Y-%m-%d %H:%M:%S%.f")
                        .map(|ndt| DateTime::from_naive_utc_and_offset(ndt, Utc))
                        .map_err(|e| anyhow::anyhow!("Failed to parse datetime: {}", e))
                };

                Ok(Target {
                    target_id: fields[0].to_string(),
                    hostname: fields[1].to_string(),
                    module: fields[2].to_string(),
                    port: fields[3].parse()?,
                    last_queued_at: parse_datetime(fields[4])?,
                    last_checked_at: parse_datetime(fields[5])?,
                    user_submitted: fields[6].parse()?,
                })
            })() {
                Ok(target) => targets.push(target),
                Err(e) => error!(%module, %line, %e, "Failed to parse target"),
            }
        }

        info!(
            %module,
            count = targets.len(),
            "Found targets to check"
        );

        Ok(targets)
    }

    pub async fn update_last_queued(&self, target_id: &str) -> Result<()> {
        let query = format!(
            "ALTER TABLE {}.targets 
             UPDATE last_queued_at = now() 
             WHERE target_id = '{}'",
            self.database, target_id
        );

        trace!(
            %target_id,
            "Updating last_queued_at"
        );

        let response = self.http_client
            .post(&self.url)
            .query(&[
                ("database", &self.database),
                ("user", &self.user),
                ("password", &self.password),
            ])
            .header("Content-Type", "text/plain")
            .body(query)
            .send()
            .await?;

        if !response.status().is_success() {
            let body = response.text().await?;
            error!(%target_id, body = %body, "Failed to update last_queued_at");
            return Err(anyhow::anyhow!("Failed to update last_queued_at: {}", body));
        }

        Ok(())
    }
} 
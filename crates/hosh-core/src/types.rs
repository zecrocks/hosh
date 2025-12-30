//! Common types used across Hosh services.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A request to check a server's status.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CheckRequest {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub check_id: Option<String>,
    #[serde(default)]
    pub user_submitted: Option<bool>,
    #[serde(default)]
    pub version: Option<String>,
}

fn default_port() -> u16 {
    443
}

impl CheckRequest {
    /// Get the check ID or a default value.
    pub fn get_check_id(&self) -> String {
        self.check_id.clone().unwrap_or_else(|| "none".to_string())
    }

    /// Check if this is a .onion address.
    pub fn is_onion(&self) -> bool {
        self.host.ends_with(".onion")
    }
}

/// The result of a server health check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub checker_module: String,
    pub hostname: String,
    pub host: String,
    pub port: u16,
    pub height: u64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub last_updated: DateTime<Utc>,
    pub ping: f64,
    pub ping_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_submitted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_data: Option<serde_json::Value>,
}

impl CheckResult {
    /// Create a new successful check result.
    pub fn success(checker_module: &str, host: &str, port: u16, height: u64, ping_ms: f64) -> Self {
        Self {
            checker_module: checker_module.to_string(),
            hostname: host.to_string(),
            host: host.to_string(),
            port,
            height,
            status: "online".to_string(),
            error: None,
            last_updated: Utc::now(),
            ping: ping_ms,
            ping_ms,
            check_id: None,
            user_submitted: None,
            server_version: None,
            error_type: None,
            error_message: None,
            additional_data: None,
        }
    }

    /// Create a new failed check result.
    pub fn failure(
        checker_module: &str,
        host: &str,
        port: u16,
        error: String,
        ping_ms: f64,
    ) -> Self {
        Self {
            checker_module: checker_module.to_string(),
            hostname: host.to_string(),
            host: host.to_string(),
            port,
            height: 0,
            status: "offline".to_string(),
            error: Some(error.clone()),
            last_updated: Utc::now(),
            ping: ping_ms,
            ping_ms,
            check_id: None,
            user_submitted: None,
            server_version: None,
            error_type: Some("connection_error".to_string()),
            error_message: Some(error),
            additional_data: None,
        }
    }

    /// Set the check ID.
    pub fn with_check_id(mut self, check_id: Option<String>) -> Self {
        self.check_id = check_id;
        self
    }

    /// Set whether this was user submitted.
    pub fn with_user_submitted(mut self, user_submitted: Option<bool>) -> Self {
        self.user_submitted = user_submitted;
        self
    }

    /// Set the server version.
    pub fn with_server_version(mut self, version: Option<String>) -> Self {
        self.server_version = version;
        self
    }

    /// Set additional data.
    pub fn with_additional_data(mut self, data: Option<serde_json::Value>) -> Self {
        self.additional_data = data;
        self
    }
}

/// ZEC-specific server information from lightwalletd.
#[derive(Debug, Clone)]
pub struct ZecServerInfo {
    pub block_height: u64,
    pub vendor: String,
    pub git_commit: String,
    pub chain_name: String,
    pub sapling_activation_height: u64,
    pub consensus_branch_id: String,
    pub taddr_support: bool,
    pub branch: String,
    pub build_date: String,
    pub build_user: String,
    pub estimated_height: u64,
    pub version: String,
    pub zcashd_build: String,
    pub zcashd_subversion: String,
    pub donation_address: String,
}

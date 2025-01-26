use axum::response::Json;
use serde::Serialize;
use std::collections::HashMap;

/// Struct to represent the health check response
#[derive(Serialize)]
pub struct HealthCheckResponse {
    pub status: String,
    pub components: HashMap<String, String>,
}

/// Handler function for the `/healthz` route
pub async fn health_check() -> Json<HealthCheckResponse> {
    let mut components = HashMap::new();
    components.insert("electrum".to_string(), "healthy".to_string());

    let response = HealthCheckResponse {
        status: "healthy".to_string(),
        components,
    };

    Json(response)
}

//! Tracing/logging initialization for Hosh services.

use tracing_subscriber::EnvFilter;

/// Initialize tracing with environment-based filtering.
///
/// Uses the `RUST_LOG` environment variable to control log levels.
/// Defaults to `info` if not set.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
}

/// Initialize tracing with a specific max level.
pub fn init_with_level(level: tracing::Level) {
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
}

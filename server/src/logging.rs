use tracing_subscriber::EnvFilter;

use crate::config::{Config, LogFormat};

pub fn init(config: &Config) {
    let filter = EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    match config.log_format {
        LogFormat::Json => {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(filter)
                .with_target(false)
                .init();
        }
        LogFormat::Pretty => {
            tracing_subscriber::fmt()
                .compact()
                .with_env_filter(filter)
                .with_target(false)
                .init();
        }
    }
}

/// Redact query strings from URLs before they are logged.
pub fn redact_url(raw: &str) -> String {
    match url::Url::parse(raw) {
        Ok(mut parsed) => {
            parsed.set_query(None);
            let _ = parsed.set_password(None);
            parsed.to_string()
        }
        Err(_) => "[unparseable-url]".to_string(),
    }
}

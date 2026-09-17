use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

/// Runtime configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    pub bind: String,
    pub port: u16,
    pub public_host: Option<String>,
    pub log_level: String,
    pub log_format: LogFormat,
    pub web_root: PathBuf,
    pub discovery_timeout: Duration,
    pub discovery_interval: Duration,
    pub request_timeout: Duration,
    pub media_session_ttl: Duration,
    pub known_cast_hosts: Vec<KnownCastHost>,
    pub known_dlna_locations: Vec<String>,
    pub cinejoy_catalog_url: Option<String>,
    pub allow_direct_urls: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Pretty,
    Json,
}

#[derive(Debug, Clone)]
pub struct KnownCastHost {
    pub host: String,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();

        Self {
            bind: env_or("CAZT_BIND", "0.0.0.0"),
            port: env_parse("CAZT_PORT", 8787),
            public_host: env_opt("CAZT_PUBLIC_HOST"),
            log_level: env_or("CAZT_LOG", "info"),
            log_format: match env_or("CAZT_LOG_FORMAT", "pretty")
                .to_ascii_lowercase()
                .as_str()
            {
                "json" => LogFormat::Json,
                _ => LogFormat::Pretty,
            },
            web_root: PathBuf::from(env_or("CAZT_WEB_ROOT", "web")),
            discovery_timeout: Duration::from_secs(env_parse("CAZT_DISCOVERY_TIMEOUT_SECS", 4)),
            discovery_interval: Duration::from_secs(env_parse("CAZT_DISCOVERY_INTERVAL_SECS", 30)),
            request_timeout: Duration::from_secs(env_parse("CAZT_REQUEST_TIMEOUT_SECS", 20)),
            media_session_ttl: Duration::from_secs(env_parse(
                "CAZT_MEDIA_SESSION_TTL_SECS",
                4 * 3600,
            )),
            known_cast_hosts: parse_cast_hosts(env_opt("CAZT_KNOWN_CAST_HOSTS")),
            known_dlna_locations: parse_csv(env_opt("CAZT_KNOWN_DLNA_LOCATIONS")),
            cinejoy_catalog_url: env_opt("CINEJOY_CATALOG_URL"),
            allow_direct_urls: env_bool("CAZT_ALLOW_DIRECT_URLS", true),
        }
    }

    pub fn listen_addr(&self) -> String {
        format!("{}:{}", self.bind, self.port)
    }

    pub fn public_summary(&self) -> PublicConfig {
        PublicConfig {
            port: self.port,
            public_host: self.public_host.clone(),
            allow_direct_urls: self.allow_direct_urls,
            cinejoy_configured: self.cinejoy_catalog_url.is_some(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PublicConfig {
    pub port: u16,
    pub public_host: Option<String>,
    pub allow_direct_urls: bool,
    pub cinejoy_configured: bool,
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn env_opt(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default,
    }
}

fn parse_csv(value: Option<String>) -> Vec<String> {
    value
        .map(|v| {
            v.split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn parse_cast_hosts(value: Option<String>) -> Vec<KnownCastHost> {
    parse_csv(value)
        .into_iter()
        .filter_map(|entry| {
            let (host, port) = if let Some((h, p)) = entry.rsplit_once(':') {
                if h.parse::<IpAddr>().is_ok() || !h.is_empty() {
                    let port = p.parse().ok()?;
                    (h.to_string(), port)
                } else {
                    (entry, 8009)
                }
            } else {
                (entry, 8009)
            };
            Some(KnownCastHost { host, port })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cast_hosts_with_optional_ports() {
        let hosts = parse_cast_hosts(Some("192.168.1.50:8009,living-room,10.0.0.8:8010".into()));
        assert_eq!(hosts.len(), 3);
        assert_eq!(hosts[0].host, "192.168.1.50");
        assert_eq!(hosts[0].port, 8009);
        assert_eq!(hosts[1].host, "living-room");
        assert_eq!(hosts[1].port, 8009);
        assert_eq!(hosts[2].port, 8010);
    }
}

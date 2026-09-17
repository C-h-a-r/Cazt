use std::collections::HashMap;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use rand::RngCore;
use url::Url;

use crate::error::{AppError, ErrorCode};
use crate::models::PlayableMedia;
use crate::proxy::ssrf::{assert_public_http_url, resolve_host_blocked};

#[derive(Debug, Clone)]
pub struct MediaSession {
    pub token: String,
    pub source: PlayableMedia,
    pub created: Instant,
    pub ttl: Duration,
    urls: HashMap<String, String>,
}

impl MediaSession {
    pub fn get_url(&self, key: &str) -> Result<String, AppError> {
        if key == "root" {
            return Ok(self.source.url.clone());
        }
        self.urls.get(key).cloned().ok_or_else(|| {
            AppError::forbidden(
                ErrorCode::ProxyDenied,
                "That media session path is not registered.",
            )
        })
    }

    pub fn expired(&self) -> bool {
        self.created.elapsed() > self.ttl
    }
}

pub struct ProxyStore {
    sessions: DashMap<String, MediaSession>,
    ttl: Duration,
}

impl ProxyStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            sessions: DashMap::new(),
            ttl,
        }
    }

    pub fn create(&self, source: PlayableMedia) -> Result<String, AppError> {
        let parsed = assert_public_http_url(&source.url)?;
        if let Some(host) = parsed.host_str() {
            // DNS is checked later at fetch time; reject literal private IPs now.
            let _ = host;
        }
        let token = random_token();
        let mut urls = HashMap::new();
        urls.insert("root".into(), source.url.clone());
        self.sessions.insert(
            token.clone(),
            MediaSession {
                token: token.clone(),
                source,
                created: Instant::now(),
                ttl: self.ttl,
                urls,
            },
        );
        Ok(token)
    }

    pub fn get(&self, token: &str) -> Result<MediaSession, AppError> {
        self.gc();
        self.sessions
            .get(token)
            .map(|entry| entry.value().clone())
            .filter(|session| !session.expired())
            .ok_or_else(|| {
                AppError::forbidden(
                    ErrorCode::ProxyDenied,
                    "That media session has expired or does not exist.",
                )
            })
    }

    pub fn register_child(&self, token: &str, url: &str) -> Result<String, AppError> {
        assert_public_http_url(url)?;
        let mut session = self.sessions.get_mut(token).ok_or_else(|| {
            AppError::forbidden(ErrorCode::ProxyDenied, "That media session does not exist.")
        })?;
        if session.expired() {
            return Err(AppError::forbidden(
                ErrorCode::ProxyDenied,
                "That media session has expired.",
            ));
        }
        if let Some((existing, _)) = session.urls.iter().find(|(_, value)| *value == url) {
            return Ok(existing.clone());
        }
        let key = format!("u{}", session.urls.len());
        session.urls.insert(key.clone(), url.to_string());
        Ok(key)
    }

    pub fn gc(&self) {
        self.sessions.retain(|_, session| !session.expired());
    }
}

pub fn random_token() -> String {
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub async fn assert_fetchable(url: &str) -> Result<Url, AppError> {
    let parsed = assert_public_http_url(url)?;
    if let Some(host) = parsed.host_str() {
        resolve_host_blocked(host).await?;
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MediaItem, MediaKind, StreamKind};

    fn media(url: &str) -> PlayableMedia {
        PlayableMedia {
            item: MediaItem {
                id: "x".into(),
                provider: "samples".into(),
                title: "Test".into(),
                subtitle: None,
                year: None,
                kind: MediaKind::Movie,
                poster: None,
                duration_seconds: None,
                playable: true,
            },
            url: url.into(),
            mime: "video/mp4".into(),
            stream_kind: StreamKind::File,
        }
    }

    #[test]
    fn session_only_serves_registered_urls() {
        let store = ProxyStore::new(Duration::from_secs(60));
        let token = store
            .create(media("https://commondatastorage.googleapis.com/sample.mp4"))
            .unwrap();
        let session = store.get(&token).unwrap();
        assert!(session.get_url("root").unwrap().starts_with("https://"));
        assert!(session.get_url("nope").is_err());
    }

    #[test]
    fn refuses_arbitrary_private_destinations() {
        let store = ProxyStore::new(Duration::from_secs(60));
        assert!(store.create(media("http://127.0.0.1/video.mp4")).is_err());
    }

    #[test]
    fn child_registration_rejects_ssrf() {
        let store = ProxyStore::new(Duration::from_secs(60));
        let token = store
            .create(media("https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8"))
            .unwrap();
        assert!(store
            .register_child(&token, "http://169.254.169.254/latest")
            .is_err());
        let key = store
            .register_child(&token, "https://test-streams.mux.dev/x36xhzz/seg.ts")
            .unwrap();
        assert_eq!(
            store.get(&token).unwrap().get_url(&key).unwrap(),
            "https://test-streams.mux.dev/x36xhzz/seg.ts"
        );
    }

    #[test]
    fn unknown_token_is_denied() {
        let store = ProxyStore::new(Duration::from_secs(60));
        assert!(store.get("not-a-real-token").is_err());
    }
}

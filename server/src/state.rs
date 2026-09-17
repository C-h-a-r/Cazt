use std::sync::Arc;

use tokio::sync::broadcast;

use crate::cast::{CastHub, CastSessionManager};
use crate::config::Config;
use crate::discovery::DeviceRegistry;
use crate::media::MediaHub;
use crate::models::ServerEvent;
use crate::proxy::session::ProxyStore;
use crate::proxy::ssrf::redirect_policy;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub devices: Arc<DeviceRegistry>,
    pub media: Arc<MediaHub>,
    pub cast: Arc<CastHub>,
    pub sessions: Arc<CastSessionManager>,
    pub proxy: Arc<ProxyStore>,
    pub events: broadcast::Sender<ServerEvent>,
    pub http: reqwest::Client,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let config = Arc::new(config);
        let (events, _) = broadcast::channel(64);
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .redirect(redirect_policy())
            .user_agent("Cazt/0.1")
            .build()
            .expect("http client");

        Self {
            devices: Arc::new(DeviceRegistry::new(
                crate::discovery::default_providers(),
                events.clone(),
            )),
            media: Arc::new(MediaHub::from_config(&config)),
            cast: Arc::new(CastHub::new()),
            sessions: Arc::new(CastSessionManager::new(events.clone())),
            proxy: Arc::new(ProxyStore::new(config.media_session_ttl)),
            events,
            http,
            config,
        }
    }
}

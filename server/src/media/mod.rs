use std::sync::Arc;

use crate::config::Config;
use crate::error::{AppError, ErrorCode};
use crate::media::cinejoy::CinejoyProvider;
use crate::media::direct::DirectUrlProvider;
use crate::media::samples::SamplesProvider;
use crate::models::{MediaItem, PlayableMedia};
use crate::proxy::ssrf::assert_public_http_url;

mod cinejoy;
pub(crate) mod direct;
mod samples;

#[async_trait::async_trait]
pub trait MediaProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    async fn search(&self, query: &str) -> Result<Vec<MediaItem>, AppError>;
    async fn get(&self, id: &str) -> Result<MediaItem, AppError>;
    async fn resolve(&self, id: &str) -> Result<PlayableMedia, AppError>;
}

pub struct MediaHub {
    providers: Vec<Arc<dyn MediaProvider>>,
}

impl MediaHub {
    pub fn from_config(config: &Config) -> Self {
        let mut providers: Vec<Arc<dyn MediaProvider>> = vec![Arc::new(SamplesProvider::new())];
        providers.push(Arc::new(CinejoyProvider::new(
            config.cinejoy_catalog_url.clone(),
        )));
        if config.allow_direct_urls {
            providers.push(Arc::new(DirectUrlProvider));
        }
        Self { providers }
    }

    pub fn with_providers(providers: Vec<Arc<dyn MediaProvider>>) -> Self {
        Self { providers }
    }

    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .iter()
            .map(|p| ProviderInfo {
                id: p.id().to_string(),
                name: p.name().to_string(),
                description: p.description().to_string(),
            })
            .collect()
    }

    pub fn get_provider(&self, id: &str) -> Result<Arc<dyn MediaProvider>, AppError> {
        self.providers
            .iter()
            .find(|p| p.id() == id)
            .cloned()
            .ok_or_else(|| AppError::not_found(ErrorCode::MediaNotFound, "Unknown media source."))
    }

    pub async fn search(
        &self,
        query: &str,
        provider: Option<&str>,
    ) -> Result<Vec<MediaItem>, AppError> {
        let mut items = Vec::new();
        for source in &self.providers {
            if let Some(filter) = provider {
                if source.id() != filter {
                    continue;
                }
            }
            items.extend(source.search(query).await?);
        }
        Ok(items)
    }

    pub async fn resolve(&self, provider: &str, id: &str) -> Result<PlayableMedia, AppError> {
        let source = self.get_provider(provider)?;
        let media = source.resolve(id).await?;
        assert_public_http_url(&media.url)?;
        Ok(media)
    }
}

#[derive(Debug, serde::Serialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

pub fn infer_mime(url: &str, explicit: Option<&str>) -> (String, crate::models::StreamKind) {
    if let Some(mime) = explicit {
        let kind = if is_hls(url, mime) {
            crate::models::StreamKind::Hls
        } else {
            crate::models::StreamKind::File
        };
        return (mime.to_string(), kind);
    }
    if url.contains(".m3u8") {
        return (
            "application/x-mpegURL".into(),
            crate::models::StreamKind::Hls,
        );
    }
    if url.contains(".webm") {
        return ("video/webm".into(), crate::models::StreamKind::File);
    }
    ("video/mp4".into(), crate::models::StreamKind::File)
}

pub fn is_hls(url: &str, mime: &str) -> bool {
    let mime = mime.to_ascii_lowercase();
    url.contains(".m3u8")
        || mime.contains("mpegurl")
        || mime.contains("application/vnd.apple.mpegurl")
}

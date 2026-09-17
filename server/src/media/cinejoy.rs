use serde::Deserialize;

use crate::error::{AppError, ErrorCode};
use crate::media::{infer_mime, MediaProvider};
use crate::models::{MediaItem, MediaKind, PlayableMedia};
use crate::proxy::ssrf::assert_public_http_url;

/// cinejoy.pk integration.
///
/// Inspection notes (2026-09-17):
/// - The site is a SvelteKit SPA (`/_app/immutable/...`) with no public
///   documented catalog or playback API.
/// - Routes such as `/dev/scrape` and a `ScrapingScreen` component indicate
///   client-side source scraping rather than a first-party media API.
/// - `/api` and similar paths return the SPA shell, not JSON.
///
/// Cazt therefore does **not** scrape cinejoy.pk, follow obfuscated player
/// endpoints, or bypass access controls. Playback is only possible when an
/// operator supplies `CINEJOY_CATALOG_URL` pointing at a permitted JSON
/// catalog of already-licensed HTTP/HLS resources.
pub struct CinejoyProvider {
    catalog_url: Option<String>,
}

impl CinejoyProvider {
    pub fn new(catalog_url: Option<String>) -> Self {
        Self { catalog_url }
    }
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    items: Vec<CatalogItem>,
}

#[derive(Debug, Deserialize)]
struct CatalogItem {
    id: String,
    title: String,
    #[serde(default)]
    subtitle: Option<String>,
    #[serde(default)]
    year: Option<u16>,
    #[serde(default)]
    poster: Option<String>,
    url: String,
    #[serde(default)]
    mime: Option<String>,
}

#[async_trait::async_trait]
impl MediaProvider for CinejoyProvider {
    fn id(&self) -> &'static str {
        "cinejoy"
    }

    fn name(&self) -> &'static str {
        "cinejoy.pk"
    }

    fn description(&self) -> &'static str {
        "Optional permitted catalog only — cinejoy.pk has no public playback API."
    }

    async fn search(&self, query: &str) -> Result<Vec<MediaItem>, AppError> {
        match self.load().await {
            Ok(items) => {
                let q = query.trim().to_ascii_lowercase();
                Ok(items
                    .into_iter()
                    .filter(|item| q.is_empty() || item.title.to_ascii_lowercase().contains(&q))
                    .collect())
            }
            Err(err) if matches!(err.code, ErrorCode::CinejoyUnavailable) => Ok(Vec::new()),
            Err(err) => Err(err),
        }
    }

    async fn get(&self, id: &str) -> Result<MediaItem, AppError> {
        self.load()
            .await?
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| {
                AppError::not_found(
                    ErrorCode::MediaNotFound,
                    "That cinejoy catalog title was not found.",
                )
            })
    }

    async fn resolve(&self, id: &str) -> Result<PlayableMedia, AppError> {
        let Some(catalog_url) = &self.catalog_url else {
            return Err(unavailable());
        };
        let items = fetch_catalog(catalog_url).await?;
        let entry = items
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| {
                AppError::not_found(
                    ErrorCode::MediaNotFound,
                    "That cinejoy catalog title was not found.",
                )
            })?;
        assert_public_http_url(&entry.url)?;
        let (mime, stream_kind) = infer_mime(&entry.url, entry.mime.as_deref());
        Ok(PlayableMedia {
            item: to_item(&entry),
            url: entry.url,
            mime,
            stream_kind,
        })
    }
}

impl CinejoyProvider {
    async fn load(&self) -> Result<Vec<MediaItem>, AppError> {
        let Some(catalog_url) = &self.catalog_url else {
            return Err(unavailable());
        };
        let items = fetch_catalog(catalog_url).await?;
        Ok(items.iter().map(to_item).collect())
    }
}

fn to_item(entry: &CatalogItem) -> MediaItem {
    MediaItem {
        id: entry.id.clone(),
        provider: "cinejoy".into(),
        title: entry.title.clone(),
        subtitle: entry.subtitle.clone(),
        year: entry.year,
        kind: MediaKind::Movie,
        poster: entry.poster.clone(),
        duration_seconds: None,
        playable: true,
    }
}

async fn fetch_catalog(catalog_url: &str) -> Result<Vec<CatalogItem>, AppError> {
    assert_public_http_url(catalog_url)?;
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::internal(e.to_string()))?
        .get(catalog_url)
        .send()
        .await
        .map_err(|e| {
            AppError::unavailable(
                ErrorCode::CinejoyUnavailable,
                format!("The configured cinejoy catalog could not be reached: {e}"),
            )
        })?;

    if !response.status().is_success() {
        return Err(AppError::unavailable(
            ErrorCode::CinejoyUnavailable,
            "The configured cinejoy catalog returned an error.",
        ));
    }

    let parsed = response.json::<CatalogFile>().await.map_err(|_| {
        AppError::unavailable(
            ErrorCode::CinejoyUnavailable,
            "The configured cinejoy catalog is not valid JSON in Cazt's catalog format.",
        )
    })?;
    Ok(parsed.items)
}

fn unavailable() -> AppError {
    AppError::unavailable(
        ErrorCode::CinejoyUnavailable,
        "cinejoy.pk has no public documented playback API. Cazt will not scrape the site. Set CINEJOY_CATALOG_URL to a permitted JSON catalog, or use the sample library.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolve_without_catalog_is_explicit() {
        let provider = CinejoyProvider::new(None);
        let err = provider.resolve("anything").await.unwrap_err();
        assert_eq!(err.code, ErrorCode::CinejoyUnavailable);
        assert!(err.message.contains("no public documented playback API"));
    }
}

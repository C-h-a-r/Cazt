use crate::error::AppError;
use crate::media::{infer_mime, MediaProvider};
use crate::models::{MediaItem, MediaKind, PlayableMedia};

/// Public-domain / Creative Commons sample streams used for development and
/// for casting when a third-party catalog cannot be resolved legally.
pub struct SamplesProvider {
    items: Vec<Sample>,
}

struct Sample {
    item: MediaItem,
    url: &'static str,
    mime: &'static str,
}

impl Default for SamplesProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SamplesProvider {
    pub fn new() -> Self {
        Self {
            items: vec![
                sample(
                    "bbb",
                    "Big Buck Bunny",
                    "Blender Foundation",
                    2008,
                    MediaKind::Movie,
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/images/BigBuckBunny.jpg",
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4",
                    "video/mp4",
                    Some(596.0),
                ),
                sample(
                    "elephants",
                    "Elephants Dream",
                    "Blender Foundation",
                    2006,
                    MediaKind::Movie,
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/images/ElephantsDream.jpg",
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ElephantsDream.mp4",
                    "video/mp4",
                    Some(654.0),
                ),
                sample(
                    "sintel",
                    "Sintel",
                    "Blender Foundation",
                    2010,
                    MediaKind::Movie,
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/images/Sintel.jpg",
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/Sintel.mp4",
                    "video/mp4",
                    Some(888.0),
                ),
                sample(
                    "tears",
                    "Tears of Steel",
                    "Blender Foundation",
                    2012,
                    MediaKind::Movie,
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/images/TearsOfSteel.jpg",
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/TearsOfSteel.mp4",
                    "video/mp4",
                    Some(734.0),
                ),
                sample(
                    "sintel-trailer",
                    "Sintel Trailer",
                    "W3C / Blender Foundation",
                    2010,
                    MediaKind::Trailer,
                    "",
                    "https://media.w3.org/2010/05/sintel/trailer_hd.mp4",
                    "video/mp4",
                    Some(52.0),
                ),
                sample(
                    "bbb-hls",
                    "Big Buck Bunny (HLS)",
                    "Mux test stream",
                    2008,
                    MediaKind::Stream,
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/images/BigBuckBunny.jpg",
                    "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8",
                    "application/x-mpegURL",
                    None,
                ),
                sample(
                    "blazes",
                    "For Bigger Blazes",
                    "Google Cast sample",
                    2015,
                    MediaKind::Short,
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/images/ForBiggerBlazes.jpg",
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerBlazes.mp4",
                    "video/mp4",
                    Some(15.0),
                ),
                sample(
                    "joyrides",
                    "For Bigger Joyrides",
                    "Google Cast sample",
                    2015,
                    MediaKind::Short,
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/images/ForBiggerJoyrides.jpg",
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerJoyrides.mp4",
                    "video/mp4",
                    Some(15.0),
                ),
                sample(
                    "escapes",
                    "For Bigger Escapes",
                    "Google Cast sample",
                    2015,
                    MediaKind::Short,
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/images/ForBiggerEscapes.jpg",
                    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerEscapes.mp4",
                    "video/mp4",
                    Some(15.0),
                ),
            ],
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sample(
    id: &str,
    title: &str,
    subtitle: &str,
    year: u16,
    kind: MediaKind,
    poster: &str,
    url: &'static str,
    mime: &'static str,
    duration_seconds: Option<f64>,
) -> Sample {
    Sample {
        item: MediaItem {
            id: id.into(),
            provider: "samples".into(),
            title: title.into(),
            subtitle: Some(subtitle.into()),
            year: Some(year),
            kind,
            poster: if poster.is_empty() {
                None
            } else {
                Some(poster.to_string())
            },
            duration_seconds,
            playable: true,
        },
        url,
        mime,
    }
}

#[async_trait::async_trait]
impl MediaProvider for SamplesProvider {
    fn id(&self) -> &'static str {
        "samples"
    }

    fn name(&self) -> &'static str {
        "Sample films"
    }

    fn description(&self) -> &'static str {
        "Open, legally redistributable test videos for casting."
    }

    async fn search(&self, query: &str) -> Result<Vec<MediaItem>, AppError> {
        let q = query.trim().to_ascii_lowercase();
        Ok(self
            .items
            .iter()
            .filter(|sample| {
                q.is_empty()
                    || sample.item.title.to_ascii_lowercase().contains(&q)
                    || sample
                        .item
                        .subtitle
                        .as_deref()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains(&q)
            })
            .map(|sample| sample.item.clone())
            .collect())
    }

    async fn get(&self, id: &str) -> Result<MediaItem, AppError> {
        self.items
            .iter()
            .find(|sample| sample.item.id == id)
            .map(|sample| sample.item.clone())
            .ok_or_else(|| {
                crate::error::AppError::not_found(
                    crate::error::ErrorCode::MediaNotFound,
                    "That title is not in the sample library.",
                )
            })
    }

    async fn resolve(&self, id: &str) -> Result<PlayableMedia, AppError> {
        let sample = self
            .items
            .iter()
            .find(|sample| sample.item.id == id)
            .ok_or_else(|| {
                crate::error::AppError::not_found(
                    crate::error::ErrorCode::MediaNotFound,
                    "That title is not in the sample library.",
                )
            })?;
        let (mime, stream_kind) = infer_mime(sample.url, Some(sample.mime));
        Ok(PlayableMedia {
            item: sample.item.clone(),
            url: sample.url.to_string(),
            mime,
            stream_kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn search_filters_titles() {
        let provider = SamplesProvider::new();
        let hits = provider.search("sintel").await.unwrap();
        assert!(hits
            .iter()
            .all(|item| item.title.to_ascii_lowercase().contains("sintel")));
        assert!(!hits.is_empty());
    }

    #[tokio::test]
    async fn resolve_returns_https_url() {
        let provider = SamplesProvider::new();
        let media = provider.resolve("bbb").await.unwrap();
        assert!(media.url.starts_with("https://"));
        assert_eq!(media.mime, "video/mp4");
    }
}

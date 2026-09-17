use crate::error::{AppError, ErrorCode};
use crate::media::{infer_mime, MediaProvider};
use crate::models::{MediaItem, MediaKind, PlayableMedia};
use crate::proxy::ssrf::assert_public_http_url;

/// Lets a user paste a public HTTP(S) media URL. Private/internal targets are
/// rejected so this cannot be used as an open SSRF primitive.
pub struct DirectUrlProvider;

#[async_trait::async_trait]
impl MediaProvider for DirectUrlProvider {
    fn id(&self) -> &'static str {
        "direct"
    }

    fn name(&self) -> &'static str {
        "Direct URL"
    }

    fn description(&self) -> &'static str {
        "Play a public HTTP or HLS URL you already have permission to use."
    }

    async fn search(&self, query: &str) -> Result<Vec<MediaItem>, AppError> {
        let query = query.trim();
        if query.starts_with("http://") || query.starts_with("https://") {
            Ok(vec![item_for(query)])
        } else {
            Ok(Vec::new())
        }
    }

    async fn get(&self, id: &str) -> Result<MediaItem, AppError> {
        Ok(item_for(&decode_id(id)?))
    }

    async fn resolve(&self, id: &str) -> Result<PlayableMedia, AppError> {
        let url = decode_id(id)?;
        assert_public_http_url(&url)?;
        let (mime, stream_kind) = infer_mime(&url, None);
        Ok(PlayableMedia {
            item: item_for(&url),
            url,
            mime,
            stream_kind,
        })
    }
}

fn item_for(url: &str) -> MediaItem {
    MediaItem {
        id: encode_id(url),
        provider: "direct".into(),
        title: title_from_url(url),
        subtitle: Some("Direct URL".into()),
        year: None,
        kind: MediaKind::Stream,
        poster: None,
        duration_seconds: None,
        playable: true,
    }
}

pub fn encode_id(url: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(url.as_bytes())
}

fn decode_id(id: &str) -> Result<String, AppError> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(id.as_bytes())
        .map_err(|_| {
            AppError::bad_request(
                ErrorCode::InvalidInput,
                "That media id is not a valid URL token.",
            )
        })?;
    String::from_utf8(bytes).map_err(|_| {
        AppError::bad_request(ErrorCode::InvalidInput, "That media id is not valid UTF-8.")
    })
}

fn title_from_url(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .path_segments()
                .and_then(|mut segs| segs.next_back())
                .map(ToOwned::to_owned)
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Direct stream".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_non_http_in_search_results_that_are_not_urls() {
        let provider = DirectUrlProvider;
        let items = provider.search("big buck bunny").await.unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn roundtrips_url_ids() {
        let url = "https://example.com/film.mp4";
        assert_eq!(decode_id(&encode_id(url)).unwrap(), url);
    }
}

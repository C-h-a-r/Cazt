use url::Url;

use crate::error::{AppError, ErrorCode};
use crate::proxy::session::ProxyStore;

/// Rewrite HLS playlists so every referenced URI goes back through the
/// session-scoped proxy. Newly discovered URLs are registered on the session
/// (and SSRF-checked) instead of being accepted as user input.
pub fn rewrite_playlist(
    store: &ProxyStore,
    token: &str,
    playlist_url: &str,
    body: &str,
    public_base: &str,
) -> Result<String, AppError> {
    let base = Url::parse(playlist_url).map_err(|_| {
        AppError::unavailable(
            ErrorCode::HlsFailed,
            "Could not parse the HLS playlist URL.",
        )
    })?;
    let mut out = String::with_capacity(body.len() + 64);
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }
        if trimmed.starts_with('#') {
            out.push_str(&rewrite_tag(store, token, &base, line, public_base)?);
            out.push('\n');
        } else {
            let resolved = resolve(&base, trimmed)?;
            let key = store.register_child(token, resolved.as_str())?;
            out.push_str(&item_url(public_base, token, &key));
            out.push('\n');
        }
    }
    if !body.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    Ok(out)
}

fn rewrite_tag(
    store: &ProxyStore,
    token: &str,
    base: &Url,
    line: &str,
    public_base: &str,
) -> Result<String, AppError> {
    if let Some(idx) = line.find("URI=\"") {
        let start = idx + 5;
        if let Some(rel_end) = line[start..].find('"') {
            let uri = &line[start..start + rel_end];
            let resolved = resolve(base, uri)?;
            let key = store.register_child(token, resolved.as_str())?;
            let mut rewritten = String::new();
            rewritten.push_str(&line[..start]);
            rewritten.push_str(&item_url(public_base, token, &key));
            rewritten.push_str(&line[start + rel_end..]);
            return Ok(rewritten);
        }
    }
    Ok(line.to_string())
}

fn resolve(base: &Url, href: &str) -> Result<Url, AppError> {
    base.join(href)
        .map_err(|_| AppError::unavailable(ErrorCode::HlsFailed, "Invalid URI in HLS playlist."))
}

pub fn item_url(public_base: &str, token: &str, key: &str) -> String {
    format!(
        "{}/media/{token}/i/{key}",
        public_base.trim_end_matches('/')
    )
}

pub fn looks_like_playlist(url: &str, content_type: Option<&str>) -> bool {
    url.contains(".m3u8")
        || content_type
            .map(|ct| {
                let ct = ct.to_ascii_lowercase();
                ct.contains("mpegurl") || ct.contains("application/vnd.apple.mpegurl")
            })
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MediaItem, MediaKind, PlayableMedia, StreamKind};
    use crate::proxy::session::ProxyStore;
    use std::time::Duration;

    #[test]
    fn rewrites_segment_and_variant_uris() {
        let store = ProxyStore::new(Duration::from_secs(60));
        let token = store
            .create(PlayableMedia {
                item: MediaItem {
                    id: "hls".into(),
                    provider: "samples".into(),
                    title: "HLS".into(),
                    subtitle: None,
                    year: None,
                    kind: MediaKind::Stream,
                    poster: None,
                    duration_seconds: None,
                    playable: true,
                },
                url: "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8".into(),
                mime: "application/x-mpegURL".into(),
                stream_kind: StreamKind::Hls,
            })
            .unwrap();
        let playlist = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=800000\n360p.m3u8\n#EXT-X-KEY:METHOD=NONE,URI=\"key.bin\"\n";
        let rewritten = rewrite_playlist(
            &store,
            &token,
            "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8",
            playlist,
            "http://192.168.1.10:8787",
        )
        .unwrap();
        assert!(rewritten.contains("http://192.168.1.10:8787/media/"));
        assert!(!rewritten.contains("360p.m3u8"));
        assert!(rewritten.contains("/i/"));
    }
}

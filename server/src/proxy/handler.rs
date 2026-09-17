use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures::TryStreamExt;
use tracing::debug;

use crate::error::{AppError, ErrorCode};
use crate::logging::redact_url;
use crate::proxy::hls::{looks_like_playlist, rewrite_playlist};
use crate::proxy::session::assert_fetchable;
use crate::state::AppState;

pub async fn proxy_root(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    proxy_named(state, token, "root".into(), headers).await
}

pub async fn proxy_item(
    State(state): State<AppState>,
    Path((token, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if !key.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::forbidden(
            ErrorCode::ProxyDenied,
            "Invalid media item id.",
        ));
    }
    proxy_named(state, token, key, headers).await
}

async fn proxy_named(
    state: AppState,
    token: String,
    key: String,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let session = state.proxy.get(&token)?;
    let url = session.get_url(&key)?;
    let parsed = assert_fetchable(&url).await?;
    debug!(url = %redact_url(parsed.as_str()), "Proxying media");

    let mut request = state.http.get(parsed.clone());
    if let Some(range) = headers.get(header::RANGE) {
        request = request.header(header::RANGE, range);
    }
    if let Some(ua) = headers.get(header::USER_AGENT) {
        request = request.header(header::USER_AGENT, ua);
    }

    let upstream = request.send().await.map_err(|err| {
        AppError::unavailable(
            ErrorCode::MediaUnavailable,
            format!("Could not fetch the source media: {err}"),
        )
    })?;

    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type = upstream
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned);

    if looks_like_playlist(parsed.as_str(), content_type.as_deref()) {
        let body = upstream.text().await.map_err(|err| {
            AppError::unavailable(
                ErrorCode::HlsFailed,
                format!("Failed to read HLS playlist: {err}"),
            )
        })?;
        let host = crate::net::advertised_host(&state.config).ok_or_else(|| {
            AppError::unavailable(
                ErrorCode::NetworkUnreachable,
                "Cazt does not know its LAN address. Set CAZT_PUBLIC_HOST so TVs can reach the media proxy.",
            )
        })?;
        let public_base = format!("http://{host}:{}", state.config.port);
        let rewritten =
            rewrite_playlist(&state.proxy, &token, parsed.as_str(), &body, &public_base)?;
        let mut response_headers = HeaderMap::new();
        response_headers.insert(
            header::CONTENT_TYPE,
            "application/vnd.apple.mpegurl".parse().unwrap(),
        );
        response_headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
        return Ok((status, response_headers, rewritten).into_response());
    }

    let mut response_headers = HeaderMap::new();
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
        header::CACHE_CONTROL,
    ] {
        if let Some(value) = upstream.headers().get(&name) {
            response_headers.insert(name, value.clone());
        }
    }
    if !response_headers.contains_key(header::ACCEPT_RANGES) {
        response_headers.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
    }

    let stream = upstream
        .bytes_stream()
        .map_err(|err| std::io::Error::other(err.to_string()));
    Ok((status, response_headers, Body::from_stream(stream)).into_response())
}

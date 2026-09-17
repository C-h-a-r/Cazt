use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::cast::{device_or_offline, resolve_playback_url, should_proxy, CastSession};
use crate::error::{AppError, ErrorCode};
use crate::models::{CastRequest, Device, PlaybackStatus, SearchQuery, SeekRequest, VolumeRequest};
use crate::state::AppState;

mod rate_limit;
mod ws;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/config", get(public_config))
        .route("/devices", get(list_devices))
        .route("/devices/refresh", post(refresh_devices))
        .route("/media/providers", get(list_providers))
        .route("/media/search", get(search_media))
        .route("/media/{provider}/{id}", get(get_media))
        .route("/cast", post(cast_media))
        .route("/playback/pause", post(pause))
        .route("/playback/resume", post(resume))
        .route("/playback/seek", post(seek))
        .route("/playback/stop", post(stop))
        .route("/playback/volume", post(volume))
        .route("/playback/status", get(playback_status))
        .route("/events", get(ws::events))
        .layer(axum::middleware::from_fn(rate_limit::limit));

    let mut app = Router::new()
        .nest("/api", api)
        .route("/media/{token}", get(crate::proxy::handler::proxy_root))
        .route(
            "/media/{token}/i/{key}",
            get(crate::proxy::handler::proxy_item),
        )
        .with_state(state.clone())
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            state.config.request_timeout + std::time::Duration::from_secs(5),
        ))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    if state.config.web_root.is_dir() {
        let index = state.config.web_root.join("index.html");
        let static_files =
            ServeDir::new(&state.config.web_root).not_found_service(ServeFile::new(index));
        app = app.fallback_service(static_files);
    } else {
        app = app.fallback(get(no_ui));
    }

    app
}

async fn no_ui() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        "Cazt API is running, but the web UI was not found. Set CAZT_WEB_ROOT or open /api/health.",
    )
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    name: &'static str,
    version: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        name: "cazt",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn public_config(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.config.public_summary())
}

#[derive(Serialize)]
struct DevicesResponse {
    devices: Vec<Device>,
    scanning: bool,
    last_error: Option<String>,
}

async fn list_devices(State(state): State<AppState>) -> Json<DevicesResponse> {
    let snap = state.devices.snapshot();
    Json(DevicesResponse {
        devices: snap.devices,
        scanning: snap.scanning,
        last_error: snap.last_error,
    })
}

async fn refresh_devices(State(state): State<AppState>) -> Json<DevicesResponse> {
    state.devices.refresh(&state.config).await;
    list_devices(State(state)).await
}

async fn list_providers(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.media.list_providers())
}

async fn search_media(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let q = query.q.unwrap_or_default();
    if q.len() > 200 {
        return Err(AppError::bad_request(
            ErrorCode::InvalidInput,
            "Search is limited to 200 characters.",
        ));
    }
    let items = state.media.search(&q, query.provider.as_deref()).await?;
    Ok(Json(items))
}

async fn get_media(
    State(state): State<AppState>,
    Path((provider, id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let item = state.media.get_provider(&provider)?.get(&id).await?;
    Ok(Json(item))
}

async fn cast_media(
    State(state): State<AppState>,
    Json(body): Json<CastRequest>,
) -> Result<Json<PlaybackStatus>, AppError> {
    if body.device_id.trim().is_empty() {
        return Err(AppError::bad_request(
            ErrorCode::InvalidInput,
            "Choose a device first.",
        ));
    }
    let device = device_or_offline(&state.devices, &body.device_id)?;
    if !device.ready {
        return Err(AppError::unavailable(
            ErrorCode::DeviceOffline,
            "That device looks offline. Refresh and try again.",
        ));
    }

    let mut media = if body.provider == "direct" {
        let url = body.url.clone().ok_or_else(|| {
            AppError::bad_request(ErrorCode::InvalidInput, "Paste a public media URL to cast.")
        })?;
        state
            .media
            .resolve("direct", &crate::media::direct::encode_id(&url))
            .await?
    } else {
        state.media.resolve(&body.provider, &body.media_id).await?
    };

    let use_proxy = should_proxy(&device, body.proxy);
    let (playback_url, using_proxy) =
        resolve_playback_url(&state.config, &state.proxy, &media, use_proxy)?;
    media.url = playback_url;

    state.sessions.start(CastSession {
        device_id: device.id.clone(),
        device_name: device.name.clone(),
        media: media.item.clone(),
        using_proxy,
    });

    if let Err(err) = state.cast.play(&device, &media).await {
        tracing::warn!(error = %err, "Playback error");
        state.sessions.fail(err.message.clone());
        state.devices.mark_offline(&device.id);
        return Err(err);
    }

    let mut status = state
        .cast
        .status(&device)
        .await
        .unwrap_or_else(|_| state.sessions.status());
    status.media = Some(media.item);
    status.using_proxy = using_proxy;
    status.active = true;
    status.device_id = Some(device.id);
    status.device_name = Some(device.name);
    if status.state == crate::models::PlaybackState::Idle {
        status.state = crate::models::PlaybackState::Playing;
    }
    state.sessions.publish(status.clone());
    Ok(Json(status))
}

async fn pause(State(state): State<AppState>) -> Result<Json<PlaybackStatus>, AppError> {
    control(
        &state,
        |cast, device| async move { cast.pause(&device).await },
    )
    .await
}

async fn resume(State(state): State<AppState>) -> Result<Json<PlaybackStatus>, AppError> {
    control(
        &state,
        |cast, device| async move { cast.resume(&device).await },
    )
    .await
}

async fn stop(State(state): State<AppState>) -> Result<Json<PlaybackStatus>, AppError> {
    let Some(session) = state.sessions.current() else {
        state.sessions.end(None);
        return Ok(Json(PlaybackStatus::idle()));
    };
    let device = device_or_offline(&state.devices, &session.device_id)?;
    if let Err(err) = state.cast.stop(&device).await {
        tracing::warn!(error = %err, "Playback error");
        state.sessions.end(Some(err.message.clone()));
        return Err(err);
    }
    state.sessions.end(None);
    Ok(Json(state.sessions.status()))
}

async fn seek(
    State(state): State<AppState>,
    Json(body): Json<SeekRequest>,
) -> Result<Json<PlaybackStatus>, AppError> {
    if !body.seconds.is_finite() || body.seconds < 0.0 {
        return Err(AppError::bad_request(
            ErrorCode::InvalidInput,
            "Seek position must be zero or greater.",
        ));
    }
    let seconds = body.seconds;
    control(&state, move |cast, device| async move {
        cast.seek(&device, seconds).await
    })
    .await
}

async fn volume(
    State(state): State<AppState>,
    Json(body): Json<VolumeRequest>,
) -> Result<Json<PlaybackStatus>, AppError> {
    if !body.level.is_finite() {
        return Err(AppError::bad_request(
            ErrorCode::InvalidInput,
            "Volume must be a number between 0 and 1.",
        ));
    }
    let level = body.level;
    control(&state, move |cast, device| async move {
        cast.set_volume(&device, level).await
    })
    .await
}

async fn playback_status(State(state): State<AppState>) -> Json<PlaybackStatus> {
    Json(refresh_status(&state).await)
}

async fn control<F, Fut>(state: &AppState, op: F) -> Result<Json<PlaybackStatus>, AppError>
where
    F: FnOnce(ArcCast, crate::models::Device) -> Fut,
    Fut: std::future::Future<Output = Result<(), AppError>>,
{
    let Some(session) = state.sessions.current() else {
        return Err(AppError::not_found(
            ErrorCode::DeviceNotFound,
            "Nothing is being cast right now.",
        ));
    };
    let device = device_or_offline(&state.devices, &session.device_id)?;
    if let Err(err) = op(ArcCast(state.cast.clone()), device.clone()).await {
        tracing::warn!(error = %err, "Playback error");
        if matches!(
            err.code,
            ErrorCode::DeviceOffline | ErrorCode::CastTimeout | ErrorCode::CastFailed
        ) {
            state.devices.mark_offline(&device.id);
        }
        state.sessions.fail(err.message.clone());
        return Err(err);
    }
    let mut status = refresh_status(state).await;
    status.media = Some(session.media);
    status.using_proxy = session.using_proxy;
    state.sessions.publish(status.clone());
    Ok(Json(status))
}

struct ArcCast(std::sync::Arc<crate::cast::CastHub>);

impl ArcCast {
    async fn pause(&self, device: &crate::models::Device) -> Result<(), AppError> {
        self.0.pause(device).await
    }
    async fn resume(&self, device: &crate::models::Device) -> Result<(), AppError> {
        self.0.resume(device).await
    }
    async fn seek(&self, device: &crate::models::Device, seconds: f64) -> Result<(), AppError> {
        self.0.seek(device, seconds).await
    }
    async fn set_volume(&self, device: &crate::models::Device, level: f32) -> Result<(), AppError> {
        self.0.set_volume(device, level).await
    }
}

async fn refresh_status(state: &AppState) -> PlaybackStatus {
    let Some(session) = state.sessions.current() else {
        return state.sessions.status();
    };
    match state.devices.get(&session.device_id) {
        Some(device) => match state.cast.status(&device).await {
            Ok(mut status) => {
                status.media = Some(session.media);
                status.using_proxy = session.using_proxy;
                status.active = true;
                status.device_id = Some(device.id);
                status.device_name = Some(device.name);
                state.sessions.publish(status.clone());
                status
            }
            Err(err) => {
                let mut status = state.sessions.status();
                status.message = Some(err.message);
                status.state = crate::models::PlaybackState::Error;
                status
            }
        },
        None => {
            state
                .sessions
                .fail("The device disappeared from the network.".into());
            state.sessions.status()
        }
    }
}

pub async fn status_loop(state: AppState) {
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(2));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        if state.sessions.current().is_some() {
            let _ = refresh_status(&state).await;
        }
    }
}

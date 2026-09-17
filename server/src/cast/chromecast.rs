use dashmap::DashMap;
use oxicast::{CastApp, CastClient, Image, MediaInfo, MediaMetadata, PlayerState, StreamType};
use tracing::{info, warn};

use crate::cast::CastProvider;
use crate::error::{AppError, ErrorCode};
use crate::models::{Device, DeviceType, PlayableMedia, PlaybackState, PlaybackStatus};

pub struct ChromecastProvider {
    clients: DashMap<String, CastClient>,
}

impl Default for ChromecastProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ChromecastProvider {
    pub fn new() -> Self {
        Self {
            clients: DashMap::new(),
        }
    }

    async fn client(&self, device: &Device) -> Result<CastClient, AppError> {
        if let Some(existing) = self.clients.get(&device.id) {
            return Ok(existing.clone());
        }
        let port = if device.port == 0 { 8009 } else { device.port };
        let client = tokio::time::timeout(
            std::time::Duration::from_secs(12),
            CastClient::connect(&device.address, port),
        )
        .await
        .map_err(|_| {
            AppError::timeout(
                ErrorCode::CastTimeout,
                "Connecting to the Chromecast timed out.",
            )
        })?
        .map_err(AppError::from)?;
        self.clients.insert(device.id.clone(), client.clone());
        Ok(client)
    }

    fn forget(&self, device_id: &str) {
        self.clients.remove(device_id);
    }
}

#[async_trait::async_trait]
impl CastProvider for ChromecastProvider {
    fn supports(&self, device: &Device) -> bool {
        device.r#type == DeviceType::Chromecast
    }

    async fn play(&self, device: &Device, media: &PlayableMedia) -> Result<(), AppError> {
        let client = match self.client(device).await {
            Ok(client) => client,
            Err(err) => {
                self.forget(&device.id);
                return Err(err);
            }
        };
        client
            .launch_app(&CastApp::DefaultMediaReceiver)
            .await
            .map_err(AppError::from)?;

        let stream_type = match media.stream_kind {
            crate::models::StreamKind::Hls => StreamType::Buffered,
            crate::models::StreamKind::File => StreamType::Buffered,
        };
        let mut info = MediaInfo::new(&media.url, &media.mime).stream_type(stream_type);
        let mut images = Vec::new();
        if let Some(poster) = &media.item.poster {
            images.push(Image {
                url: poster.clone(),
                width: None,
                height: None,
            });
        }
        info = info.metadata(MediaMetadata::Movie {
            title: Some(media.item.title.clone()),
            subtitle: media.item.subtitle.clone(),
            studio: None,
            images,
        });
        if let Some(duration) = media.item.duration_seconds {
            info = info.duration(duration);
        }

        client
            .load_media(&info, true, 0.0, None)
            .await
            .map_err(AppError::from)?;
        info!(device = %device.name, title = %media.item.title, "Cast session started");
        Ok(())
    }

    async fn pause(&self, device: &Device) -> Result<(), AppError> {
        self.client(device)
            .await?
            .pause()
            .await
            .map(|_| ())
            .map_err(AppError::from)
    }

    async fn resume(&self, device: &Device) -> Result<(), AppError> {
        self.client(device)
            .await?
            .play()
            .await
            .map(|_| ())
            .map_err(AppError::from)
    }

    async fn seek(&self, device: &Device, seconds: f64) -> Result<(), AppError> {
        self.client(device)
            .await?
            .seek(seconds.max(0.0))
            .await
            .map(|_| ())
            .map_err(AppError::from)
    }

    async fn stop(&self, device: &Device) -> Result<(), AppError> {
        let result = self.client(device).await?.stop_media().await.map(|_| ());
        info!(device = %device.name, "Cast session ended");
        self.forget(&device.id);
        result.map_err(AppError::from)
    }

    async fn set_volume(&self, device: &Device, level: f32) -> Result<(), AppError> {
        self.client(device)
            .await?
            .set_volume(level)
            .await
            .map(|_| ())
            .map_err(AppError::from)
    }

    async fn status(&self, device: &Device) -> Result<PlaybackStatus, AppError> {
        let client = self.client(device).await.map_err(|err| {
            warn!(device = %device.name, error = %err, "Playback status cannot be retrieved");
            AppError::unavailable(
                ErrorCode::StatusUnavailable,
                "Could not read Chromecast playback status.",
            )
        })?;
        let media = client.media_status().await.map_err(|_| {
            AppError::unavailable(
                ErrorCode::StatusUnavailable,
                "Could not read Chromecast playback status.",
            )
        })?;
        let receiver = client.receiver_status().await.ok();
        let mut status = PlaybackStatus::idle();
        status.active = true;
        status.device_id = Some(device.id.clone());
        status.device_name = Some(device.name.clone());
        if let Some(media) = media {
            status.state = match media.player_state {
                PlayerState::Playing => PlaybackState::Playing,
                PlayerState::Paused => PlaybackState::Paused,
                PlayerState::Buffering => PlaybackState::Buffering,
                PlayerState::Idle => PlaybackState::Idle,
                _ => PlaybackState::Buffering,
            };
            status.position_seconds = Some(media.current_time);
            status.duration_seconds = media.duration;
            status.volume = Some(media.volume.level);
            status.muted = Some(media.volume.muted);
        } else if let Some(receiver) = receiver {
            status.volume = Some(receiver.volume.level);
            status.muted = Some(receiver.volume.muted);
            status.state = PlaybackState::Idle;
        }
        Ok(status)
    }
}

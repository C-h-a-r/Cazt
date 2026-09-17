use std::sync::Arc;

use crate::config::Config;
use crate::discovery::DeviceRegistry;
use crate::error::{AppError, ErrorCode};
use crate::models::{Device, DeviceType, PlayableMedia};
use crate::net::advertised_host;
use crate::proxy::session::ProxyStore;

mod chromecast;
mod dlna;
mod session;

pub use session::{CastSession, CastSessionManager};

#[async_trait::async_trait]
pub trait CastProvider: Send + Sync {
    fn supports(&self, device: &Device) -> bool;
    async fn play(&self, device: &Device, media: &PlayableMedia) -> Result<(), AppError>;
    async fn pause(&self, device: &Device) -> Result<(), AppError>;
    async fn resume(&self, device: &Device) -> Result<(), AppError>;
    async fn seek(&self, device: &Device, seconds: f64) -> Result<(), AppError>;
    async fn stop(&self, device: &Device) -> Result<(), AppError>;
    async fn set_volume(&self, device: &Device, level: f32) -> Result<(), AppError>;
    async fn status(&self, device: &Device) -> Result<crate::models::PlaybackStatus, AppError>;
}

pub struct CastHub {
    providers: Vec<Arc<dyn CastProvider>>,
}

impl Default for CastHub {
    fn default() -> Self {
        Self::new()
    }
}

impl CastHub {
    pub fn new() -> Self {
        Self {
            providers: vec![
                Arc::new(chromecast::ChromecastProvider::new()),
                Arc::new(dlna::DlnaProvider::new()),
            ],
        }
    }

    pub fn with_providers(providers: Vec<Arc<dyn CastProvider>>) -> Self {
        Self { providers }
    }

    fn provider_for(&self, device: &Device) -> Result<Arc<dyn CastProvider>, AppError> {
        self.providers
            .iter()
            .find(|p| p.supports(device))
            .cloned()
            .ok_or_else(|| {
                AppError::unavailable(
                    ErrorCode::UnsupportedControl,
                    "Cazt does not have a casting provider for that device.",
                )
            })
    }

    pub async fn play(&self, device: &Device, media: &PlayableMedia) -> Result<(), AppError> {
        self.provider_for(device)?.play(device, media).await
    }

    pub async fn pause(&self, device: &Device) -> Result<(), AppError> {
        require(device.capabilities.pause, "pause")?;
        self.provider_for(device)?.pause(device).await
    }

    pub async fn resume(&self, device: &Device) -> Result<(), AppError> {
        require(device.capabilities.play, "resume")?;
        self.provider_for(device)?.resume(device).await
    }

    pub async fn seek(&self, device: &Device, seconds: f64) -> Result<(), AppError> {
        require(device.capabilities.seek, "seek")?;
        self.provider_for(device)?.seek(device, seconds).await
    }

    pub async fn stop(&self, device: &Device) -> Result<(), AppError> {
        require(device.capabilities.stop, "stop")?;
        self.provider_for(device)?.stop(device).await
    }

    pub async fn set_volume(&self, device: &Device, level: f32) -> Result<(), AppError> {
        require(device.capabilities.volume, "volume")?;
        self.provider_for(device)?
            .set_volume(device, level.clamp(0.0, 1.0))
            .await
    }

    pub async fn status(&self, device: &Device) -> Result<crate::models::PlaybackStatus, AppError> {
        self.provider_for(device)?.status(device).await
    }
}

fn require(supported: bool, name: &str) -> Result<(), AppError> {
    if supported {
        Ok(())
    } else {
        Err(AppError::bad_request(
            ErrorCode::UnsupportedControl,
            format!("This device does not support {name}."),
        ))
    }
}

/// Build a LAN-reachable proxy URL, or the original source URL.
pub fn resolve_playback_url(
    config: &Config,
    proxy: &ProxyStore,
    media: &PlayableMedia,
    use_proxy: bool,
) -> Result<(String, bool), AppError> {
    if !use_proxy {
        return Ok((media.url.clone(), false));
    }
    let host = advertised_host(config).ok_or_else(|| {
        AppError::unavailable(
            ErrorCode::NetworkUnreachable,
            "Cazt cannot advertise a LAN address for the media proxy. Set CAZT_PUBLIC_HOST to this machine's LAN IP (not localhost).",
        )
    })?;
    let token = proxy.create(media.clone())?;
    tracing::info!(
        token_prefix = &token[..8.min(token.len())],
        "Media proxy session created"
    );
    Ok((format!("http://{host}:{}/media/{token}", config.port), true))
}

pub fn should_proxy(device: &Device, requested: Option<bool>) -> bool {
    requested.unwrap_or(matches!(device.r#type, DeviceType::Dlna))
}

pub fn device_or_offline(registry: &DeviceRegistry, id: &str) -> Result<Device, AppError> {
    registry.get(id).ok_or_else(|| {
        AppError::not_found(
            ErrorCode::DeviceNotFound,
            "That device is no longer available. Try refreshing.",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Capabilities, Device};

    struct MockProvider;

    #[async_trait::async_trait]
    impl CastProvider for MockProvider {
        fn supports(&self, device: &Device) -> bool {
            device.id == "ok"
        }
        async fn play(&self, _device: &Device, _media: &PlayableMedia) -> Result<(), AppError> {
            Ok(())
        }
        async fn pause(&self, _device: &Device) -> Result<(), AppError> {
            Ok(())
        }
        async fn resume(&self, _device: &Device) -> Result<(), AppError> {
            Ok(())
        }
        async fn seek(&self, _device: &Device, _seconds: f64) -> Result<(), AppError> {
            Ok(())
        }
        async fn stop(&self, _device: &Device) -> Result<(), AppError> {
            Ok(())
        }
        async fn set_volume(&self, _device: &Device, _level: f32) -> Result<(), AppError> {
            Ok(())
        }
        async fn status(
            &self,
            _device: &Device,
        ) -> Result<crate::models::PlaybackStatus, AppError> {
            Ok(crate::models::PlaybackStatus::idle())
        }
    }

    fn device(id: &str) -> Device {
        Device {
            id: id.into(),
            name: "TV".into(),
            r#type: DeviceType::Chromecast,
            manufacturer: "Google".into(),
            model: "Chromecast".into(),
            address: "192.168.1.50".into(),
            port: 8009,
            capabilities: Capabilities::chromecast(),
            location: None,
            ready: true,
        }
    }

    #[tokio::test]
    async fn unknown_device_type_is_rejected() {
        let hub = CastHub::with_providers(vec![Arc::new(MockProvider)]);
        let err = hub.pause(&device("missing")).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedControl);
    }
}

use rupnp::ssdp::URN;
use tracing::{info, warn};

use crate::cast::CastProvider;
use crate::error::{AppError, ErrorCode};
use crate::models::{Device, DeviceType, PlayableMedia, PlaybackState, PlaybackStatus};

const AV_TRANSPORT: URN = URN::service("schemas-upnp-org", "AVTransport", 1);
const AV_TRANSPORT_2: URN = URN::service("schemas-upnp-org", "AVTransport", 2);
const RENDERING_CONTROL: URN = URN::service("schemas-upnp-org", "RenderingControl", 1);

#[derive(Default)]
pub struct DlnaProvider;

impl DlnaProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl CastProvider for DlnaProvider {
    fn supports(&self, device: &Device) -> bool {
        device.r#type == DeviceType::Dlna
    }

    async fn play(&self, device: &Device, media: &PlayableMedia) -> Result<(), AppError> {
        let (upnp, av) = av_service(device).await?;
        let didl = didl_lite(&media.item.title, &media.url, &media.mime);
        let payload = format!(
            "<InstanceID>0</InstanceID><CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",
            xml_escape(&media.url),
            xml_escape(&didl)
        );
        av.action(upnp.url(), "SetAVTransportURI", &payload)
            .await
            .map_err(AppError::from)?;
        av.action(
            upnp.url(),
            "Play",
            "<InstanceID>0</InstanceID><Speed>1</Speed>",
        )
        .await
        .map_err(|err| {
            AppError::unavailable(
                ErrorCode::DlnaRejected,
                format!("The TV accepted the URI but refused to play: {err}"),
            )
        })?;
        info!(device = %device.name, title = %media.item.title, "Cast session started");
        Ok(())
    }

    async fn pause(&self, device: &Device) -> Result<(), AppError> {
        let (upnp, av) = av_service(device).await?;
        av.action(upnp.url(), "Pause", "<InstanceID>0</InstanceID>")
            .await
            .map(|_| ())
            .map_err(AppError::from)
    }

    async fn resume(&self, device: &Device) -> Result<(), AppError> {
        let (upnp, av) = av_service(device).await?;
        av.action(
            upnp.url(),
            "Play",
            "<InstanceID>0</InstanceID><Speed>1</Speed>",
        )
        .await
        .map(|_| ())
        .map_err(AppError::from)
    }

    async fn seek(&self, device: &Device, seconds: f64) -> Result<(), AppError> {
        let (upnp, av) = av_service(device).await?;
        let target = format_clock(seconds);
        let payload =
            format!("<InstanceID>0</InstanceID><Unit>REL_TIME</Unit><Target>{target}</Target>");
        av.action(upnp.url(), "Seek", &payload)
            .await
            .map(|_| ())
            .map_err(AppError::from)
    }

    async fn stop(&self, device: &Device) -> Result<(), AppError> {
        let (upnp, av) = av_service(device).await?;
        let result = av
            .action(upnp.url(), "Stop", "<InstanceID>0</InstanceID>")
            .await
            .map(|_| ())
            .map_err(AppError::from);
        info!(device = %device.name, "Cast session ended");
        result
    }

    async fn set_volume(&self, device: &Device, level: f32) -> Result<(), AppError> {
        let upnp = load_device(device).await?;
        let rc = upnp
            .find_service(&RENDERING_CONTROL)
            .cloned()
            .ok_or_else(|| {
                AppError::bad_request(
                    ErrorCode::UnsupportedControl,
                    "This DLNA device does not expose volume control.",
                )
            })?;
        let volume = (level.clamp(0.0, 1.0) * 100.0).round() as u8;
        let payload = format!(
            "<InstanceID>0</InstanceID><Channel>Master</Channel><DesiredVolume>{volume}</DesiredVolume>"
        );
        rc.action(upnp.url(), "SetVolume", &payload)
            .await
            .map(|_| ())
            .map_err(AppError::from)
    }

    async fn status(&self, device: &Device) -> Result<PlaybackStatus, AppError> {
        let (upnp, av) = av_service(device).await.map_err(|err| {
            warn!(device = %device.name, error = %err, "Playback status cannot be retrieved");
            AppError::unavailable(
                ErrorCode::StatusUnavailable,
                "Could not read DLNA playback status.",
            )
        })?;
        let transport = av
            .action(upnp.url(), "GetTransportInfo", "<InstanceID>0</InstanceID>")
            .await
            .unwrap_or_default();
        let position = av
            .action(upnp.url(), "GetPositionInfo", "<InstanceID>0</InstanceID>")
            .await
            .unwrap_or_default();
        let state = transport
            .get("CurrentTransportState")
            .map(|s| match s.to_ascii_uppercase().as_str() {
                "PLAYING" => PlaybackState::Playing,
                "PAUSED_PLAYBACK" | "PAUSED" => PlaybackState::Paused,
                "TRANSITIONING" => PlaybackState::Buffering,
                "STOPPED" | "NO_MEDIA_PRESENT" => PlaybackState::Stopped,
                _ => PlaybackState::Idle,
            })
            .unwrap_or(PlaybackState::Idle);

        let mut status = PlaybackStatus::idle();
        status.active = true;
        status.device_id = Some(device.id.clone());
        status.device_name = Some(device.name.clone());
        status.state = state;
        status.position_seconds = position.get("RelTime").and_then(|v| parse_clock(v));
        status.duration_seconds = position.get("TrackDuration").and_then(|v| parse_clock(v));

        if let Some(rc) = upnp.find_service(&RENDERING_CONTROL) {
            if let Ok(volume) = rc
                .action(
                    upnp.url(),
                    "GetVolume",
                    "<InstanceID>0</InstanceID><Channel>Master</Channel>",
                )
                .await
            {
                if let Some(value) = volume
                    .get("CurrentVolume")
                    .and_then(|v| v.parse::<f32>().ok())
                {
                    status.volume = Some((value / 100.0).clamp(0.0, 1.0));
                }
            }
        }
        Ok(status)
    }
}

async fn load_device(device: &Device) -> Result<rupnp::Device, AppError> {
    let location = device.location.as_deref().ok_or_else(|| {
        AppError::unavailable(
            ErrorCode::DeviceOffline,
            "This DLNA device has no description URL.",
        )
    })?;
    let uri = location.parse().map_err(|_| {
        AppError::unavailable(ErrorCode::DeviceOffline, "Invalid DLNA description URL.")
    })?;
    tokio::time::timeout(
        std::time::Duration::from_secs(8),
        rupnp::Device::from_url(uri),
    )
    .await
    .map_err(|_| {
        AppError::timeout(
            ErrorCode::CastTimeout,
            "The DLNA device stopped responding.",
        )
    })?
    .map_err(|_| AppError::unavailable(ErrorCode::DeviceOffline, "The DLNA device went offline."))
}

async fn av_service(device: &Device) -> Result<(rupnp::Device, rupnp::Service), AppError> {
    let upnp = load_device(device).await?;
    let service = upnp
        .find_service(&AV_TRANSPORT)
        .cloned()
        .or_else(|| upnp.find_service(&AV_TRANSPORT_2).cloned())
        .ok_or_else(|| {
            AppError::unavailable(
                ErrorCode::UnsupportedControl,
                "This device no longer exposes AVTransport, so Cazt cannot control it.",
            )
        })?;
    Ok((upnp, service))
}

fn didl_lite(title: &str, url: &str, mime: &str) -> String {
    format!(
        r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/"><item id="0" parentID="-1" restricted="1"><dc:title>{}</dc:title><upnp:class>object.item.videoItem</upnp:class><res protocolInfo="http-get:*:{}:*">{}</res></item></DIDL-Lite>"#,
        xml_escape(title),
        xml_escape(mime),
        xml_escape(url)
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn format_clock(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{h}:{m:02}:{s:02}")
}

fn parse_clock(value: &str) -> Option<f64> {
    if value == "NOT_IMPLEMENTED" || value.is_empty() {
        return None;
    }
    let mut parts = value.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let s: f64 = parts.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_roundtrip() {
        assert_eq!(format_clock(125.0), "0:02:05");
        assert_eq!(parse_clock("1:02:03"), Some(3723.0));
        assert_eq!(parse_clock("NOT_IMPLEMENTED"), None);
    }

    #[test]
    fn xml_escapes_url() {
        let xml = xml_escape(r#"http://x/a&b<>"c"#);
        assert!(xml.contains("&amp;"));
        assert!(xml.contains("&lt;"));
    }
}

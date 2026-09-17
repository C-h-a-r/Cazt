use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Chromecast,
    Dlna,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Capabilities {
    pub play: bool,
    pub pause: bool,
    pub seek: bool,
    pub stop: bool,
    pub volume: bool,
}

impl Capabilities {
    pub fn chromecast() -> Self {
        Self {
            play: true,
            pause: true,
            seek: true,
            stop: true,
            volume: true,
        }
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            play: self.play || other.play,
            pause: self.pause || other.pause,
            seek: self.seek || other.seek,
            stop: self.stop || other.stop,
            volume: self.volume || other.volume,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub r#type: DeviceType,
    pub manufacturer: String,
    pub model: String,
    pub address: String,
    pub port: u16,
    pub capabilities: Capabilities,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaItem {
    pub id: String,
    pub provider: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub year: Option<u16>,
    pub kind: MediaKind,
    pub poster: Option<String>,
    pub duration_seconds: Option<f64>,
    pub playable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Movie,
    Short,
    Trailer,
    Stream,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayableMedia {
    pub item: MediaItem,
    pub url: String,
    pub mime: String,
    pub stream_kind: StreamKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamKind {
    File,
    Hls,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlaybackStatus {
    pub active: bool,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub media: Option<MediaItem>,
    pub state: PlaybackState,
    pub position_seconds: Option<f64>,
    pub duration_seconds: Option<f64>,
    pub volume: Option<f32>,
    pub muted: Option<bool>,
    pub using_proxy: bool,
    pub message: Option<String>,
}

impl PlaybackStatus {
    pub fn idle() -> Self {
        Self {
            active: false,
            device_id: None,
            device_name: None,
            media: None,
            state: PlaybackState::Idle,
            position_seconds: None,
            duration_seconds: None,
            volume: None,
            muted: None,
            using_proxy: false,
            message: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackState {
    Idle,
    Connecting,
    Playing,
    Paused,
    Buffering,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum ServerEvent {
    Devices {
        devices: Vec<Device>,
        scanning: bool,
        last_error: Option<String>,
    },
    Playback {
        status: PlaybackStatus,
    },
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct CastRequest {
    pub device_id: String,
    pub provider: String,
    pub media_id: String,
    #[serde(default)]
    pub proxy: Option<bool>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeekRequest {
    pub seconds: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VolumeRequest {
    pub level: f32,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub provider: Option<String>,
}

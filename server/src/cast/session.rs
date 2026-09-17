use parking_lot::Mutex;
use tokio::sync::broadcast;
use tracing::info;

use crate::models::{MediaItem, PlaybackState, PlaybackStatus, ServerEvent};

#[derive(Debug, Clone)]
pub struct CastSession {
    pub device_id: String,
    pub device_name: String,
    pub media: MediaItem,
    pub using_proxy: bool,
}

pub struct CastSessionManager {
    current: Mutex<Option<CastSession>>,
    last_status: Mutex<PlaybackStatus>,
    events: broadcast::Sender<ServerEvent>,
}

impl CastSessionManager {
    pub fn new(events: broadcast::Sender<ServerEvent>) -> Self {
        Self {
            current: Mutex::new(None),
            last_status: Mutex::new(PlaybackStatus::idle()),
            events,
        }
    }

    pub fn start(&self, session: CastSession) {
        info!(device = %session.device_name, title = %session.media.title, "Cast session started");
        *self.current.lock() = Some(session.clone());
        let mut status = PlaybackStatus::idle();
        status.active = true;
        status.device_id = Some(session.device_id);
        status.device_name = Some(session.device_name);
        status.media = Some(session.media);
        status.state = PlaybackState::Connecting;
        status.using_proxy = session.using_proxy;
        self.publish(status);
    }

    pub fn current(&self) -> Option<CastSession> {
        self.current.lock().clone()
    }

    pub fn status(&self) -> PlaybackStatus {
        self.last_status.lock().clone()
    }

    pub fn publish(&self, status: PlaybackStatus) {
        *self.last_status.lock() = status.clone();
        let _ = self.events.send(ServerEvent::Playback { status });
    }

    pub fn end(&self, message: Option<String>) {
        if let Some(session) = self.current.lock().take() {
            info!(device = %session.device_name, "Cast session ended");
        }
        let mut status = PlaybackStatus::idle();
        status.message = message;
        status.state = PlaybackState::Stopped;
        self.publish(status);
    }

    pub fn fail(&self, message: String) {
        let mut status = self.status();
        status.state = PlaybackState::Error;
        status.message = Some(message.clone());
        self.publish(status);
        let _ = self.events.send(ServerEvent::Error {
            code: "PLAYBACK_ERROR".into(),
            message,
        });
    }
}

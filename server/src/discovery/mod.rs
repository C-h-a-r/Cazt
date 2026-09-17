use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::config::Config;
use crate::discovery::dedup::{deduplicate, RawDevice};
use crate::models::{Device, ServerEvent};

mod chromecast;
mod dedup;
mod dlna;

pub use chromecast::ChromecastDiscovery;
pub use dlna::DlnaDiscovery;

#[async_trait::async_trait]
pub trait DiscoveryProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn scan(&self, config: &Config) -> Result<Vec<RawDevice>, String>;
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoverySnapshot {
    pub devices: Vec<Device>,
    pub scanning: bool,
    pub last_error: Option<String>,
    pub last_scan: Option<String>,
}

struct Inner {
    devices: Vec<Device>,
    raw: Vec<RawDevice>,
    scanning: bool,
    last_error: Option<String>,
    last_scan: Option<Instant>,
}

pub struct DeviceRegistry {
    inner: RwLock<Inner>,
    providers: Vec<Arc<dyn DiscoveryProvider>>,
    events: broadcast::Sender<ServerEvent>,
}

impl DeviceRegistry {
    pub fn new(
        providers: Vec<Arc<dyn DiscoveryProvider>>,
        events: broadcast::Sender<ServerEvent>,
    ) -> Self {
        Self {
            inner: RwLock::new(Inner {
                devices: Vec::new(),
                raw: Vec::new(),
                scanning: false,
                last_error: None,
                last_scan: None,
            }),
            providers,
            events,
        }
    }

    pub fn snapshot(&self) -> DiscoverySnapshot {
        let inner = self.inner.read();
        DiscoverySnapshot {
            devices: inner.devices.clone(),
            scanning: inner.scanning,
            last_error: inner.last_error.clone(),
            last_scan: inner
                .last_scan
                .map(|t| format!("{}s ago", t.elapsed().as_secs())),
        }
    }

    pub fn devices(&self) -> Vec<Device> {
        self.inner.read().devices.clone()
    }

    pub fn get(&self, id: &str) -> Option<Device> {
        self.inner
            .read()
            .devices
            .iter()
            .find(|d| d.id == id)
            .cloned()
    }

    pub fn raw(&self, id: &str) -> Option<RawDevice> {
        self.inner
            .read()
            .raw
            .iter()
            .find(|d| d.device.id == id)
            .cloned()
    }

    pub fn mark_offline(&self, id: &str) {
        let mut inner = self.inner.write();
        if let Some(device) = inner.devices.iter_mut().find(|d| d.id == id) {
            if device.ready {
                device.ready = false;
                info!(device_id = %id, name = %device.name, "Device removed");
            }
        }
        if let Some(raw) = inner.raw.iter_mut().find(|d| d.device.id == id) {
            raw.device.ready = false;
        }
        let devices = inner.devices.clone();
        let scanning = inner.scanning;
        let last_error = inner.last_error.clone();
        drop(inner);
        let _ = self.events.send(ServerEvent::Devices {
            devices,
            scanning,
            last_error,
        });
    }

    pub async fn refresh(&self, config: &Config) {
        {
            let mut inner = self.inner.write();
            inner.scanning = true;
            let devices = inner.devices.clone();
            let last_error = inner.last_error.clone();
            drop(inner);
            let _ = self.events.send(ServerEvent::Devices {
                devices,
                scanning: true,
                last_error,
            });
        }

        let mut collected = Vec::new();
        let mut errors = Vec::new();
        let budget = config.discovery_timeout + std::time::Duration::from_secs(3);

        let scans = self.providers.iter().map(|provider| {
            let provider = provider.clone();
            async move {
                let name = provider.name();
                let result = tokio::time::timeout(budget, provider.scan(config)).await;
                (name, result)
            }
        });
        let results = futures::future::join_all(scans).await;

        for (name, result) in results {
            match result {
                Ok(Ok(found)) => {
                    for device in &found {
                        if !self
                            .inner
                            .read()
                            .raw
                            .iter()
                            .any(|existing| existing.device.id == device.device.id)
                        {
                            info!(
                                device_id = %device.device.id,
                                name = %device.device.name,
                                protocol = name,
                                address = %device.device.address,
                                "Device discovered"
                            );
                        }
                    }
                    collected.extend(found);
                }
                Ok(Err(err)) => {
                    warn!(provider = name, error = %err, "Network discovery error");
                    errors.push(format!("{name}: {err}"));
                }
                Err(_) => {
                    warn!(provider = name, "Network discovery error");
                    errors.push(format!("{name}: scan timed out"));
                }
            }
        }

        let previous: HashMap<String, Device> = self
            .inner
            .read()
            .devices
            .iter()
            .cloned()
            .map(|d| (d.id.clone(), d))
            .collect();

        let merged = deduplicate(collected);
        for old in previous.values() {
            if !merged.iter().any(|d| d.device.id == old.id) {
                info!(device_id = %old.id, name = %old.name, "Device removed");
            }
        }

        let devices: Vec<Device> = merged.iter().map(|d| d.device.clone()).collect();
        let last_error = if errors.is_empty() {
            None
        } else {
            Some(errors.join("; "))
        };

        {
            let mut inner = self.inner.write();
            inner.raw = merged;
            inner.devices = devices.clone();
            inner.scanning = false;
            inner.last_error = last_error.clone();
            inner.last_scan = Some(Instant::now());
        }

        let _ = self.events.send(ServerEvent::Devices {
            devices,
            scanning: false,
            last_error,
        });
    }
}

pub async fn run_loop(registry: Arc<DeviceRegistry>, config: Config) {
    registry.refresh(&config).await;
    let mut ticker = tokio::time::interval(config.discovery_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        registry.refresh(&config).await;
    }
}

pub fn stable_id(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    hex::encode(&digest[..12])
}

pub fn default_providers() -> Vec<Arc<dyn DiscoveryProvider>> {
    vec![Arc::new(ChromecastDiscovery), Arc::new(DlnaDiscovery)]
}

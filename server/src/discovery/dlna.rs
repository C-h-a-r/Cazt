use std::collections::HashSet;
use std::time::Duration;

use futures::TryStreamExt;
use rupnp::ssdp::{SearchTarget, URN};
use tracing::debug;

use crate::config::Config;
use crate::discovery::dedup::RawDevice;
use crate::discovery::{stable_id, DiscoveryProvider};
use crate::models::{Capabilities, Device, DeviceType};

const AV_TRANSPORT: URN = URN::service("schemas-upnp-org", "AVTransport", 1);
const AV_TRANSPORT_2: URN = URN::service("schemas-upnp-org", "AVTransport", 2);
const RENDERING_CONTROL: URN = URN::service("schemas-upnp-org", "RenderingControl", 1);
const MEDIA_RENDERER: URN = URN::device("schemas-upnp-org", "MediaRenderer", 1);

pub struct DlnaDiscovery;

#[async_trait::async_trait]
impl DiscoveryProvider for DlnaDiscovery {
    fn name(&self) -> &'static str {
        "dlna"
    }

    async fn scan(&self, config: &Config) -> Result<Vec<RawDevice>, String> {
        let mut seen = HashSet::new();
        let mut devices = Vec::new();
        let mut last_error = None;

        for target in [
            SearchTarget::URN(MEDIA_RENDERER),
            SearchTarget::URN(AV_TRANSPORT),
        ] {
            match collect(target, config.discovery_timeout, &mut seen).await {
                Ok(found) => devices.extend(found),
                Err(err) => {
                    debug!(error = %err, "DLNA SSDP scan failed");
                    last_error = Some(err);
                }
            }
        }

        for location in &config.known_dlna_locations {
            if seen.contains(location) {
                continue;
            }
            match load_location(location).await {
                Ok(Some(device)) => {
                    seen.insert(location.clone());
                    devices.push(device);
                }
                Ok(None) => {}
                Err(err) => last_error = Some(err),
            }
        }

        if devices.is_empty() {
            if let Some(err) = last_error {
                return Err(err);
            }
        }

        Ok(devices)
    }
}

async fn collect(
    target: SearchTarget,
    timeout: Duration,
    seen: &mut HashSet<String>,
) -> Result<Vec<RawDevice>, String> {
    let stream = rupnp::discover(&target, timeout, None)
        .await
        .map_err(|e| e.to_string())?;
    futures::pin_mut!(stream);

    let mut devices = Vec::new();
    while let Some(device) = stream.try_next().await.map_err(|e| e.to_string())? {
        let location = device.url().to_string();
        if !seen.insert(location.clone()) {
            continue;
        }
        if let Some(raw) = from_upnp(device) {
            devices.push(raw);
        }
    }
    Ok(devices)
}

async fn load_location(location: &str) -> Result<Option<RawDevice>, String> {
    let uri = location
        .parse()
        .map_err(|e: http::uri::InvalidUri| e.to_string())?;
    let device = rupnp::Device::from_url(uri)
        .await
        .map_err(|e| e.to_string())?;
    Ok(from_upnp(device))
}

fn from_upnp(device: rupnp::Device) -> Option<RawDevice> {
    let has_av = device.find_service(&AV_TRANSPORT).is_some()
        || device.find_service(&AV_TRANSPORT_2).is_some();
    if !has_av {
        return None;
    }

    let volume = device.find_service(&RENDERING_CONTROL).is_some();
    let address = device.url().host().unwrap_or("unknown").to_string();
    let port = device.url().port_u16().unwrap_or(0);
    let udn = device.udn().to_string();
    let id = if udn.is_empty() {
        format!("dlna-{}", stable_id(&[&device.url().to_string()]))
    } else {
        format!("dlna-{}", udn.trim_start_matches("uuid:"))
    };

    Some(RawDevice {
        device: Device {
            id,
            name: device.friendly_name().to_string(),
            r#type: DeviceType::Dlna,
            manufacturer: device.manufacturer().to_string(),
            model: device.model_name().to_string(),
            address,
            port,
            capabilities: Capabilities {
                play: true,
                pause: true,
                seek: true,
                stop: true,
                volume,
            },
            location: Some(device.url().to_string()),
            ready: true,
        },
        uuid: Some(udn),
        dlna_location: Some(device.url().to_string()),
        cast_port: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avtransport_urn_is_stable() {
        assert!(AV_TRANSPORT.to_string().contains("AVTransport"));
        assert!(MEDIA_RENDERER.to_string().contains("MediaRenderer"));
    }
}

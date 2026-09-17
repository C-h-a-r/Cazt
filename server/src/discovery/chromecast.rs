use oxicast::DeviceInfo;
use tracing::debug;

use crate::config::Config;
use crate::discovery::dedup::RawDevice;
use crate::discovery::{stable_id, DiscoveryProvider};
use crate::models::{Capabilities, Device, DeviceType};

pub struct ChromecastDiscovery;

#[async_trait::async_trait]
impl DiscoveryProvider for ChromecastDiscovery {
    fn name(&self) -> &'static str {
        "chromecast"
    }

    async fn scan(&self, config: &Config) -> Result<Vec<RawDevice>, String> {
        let mut devices = Vec::new();

        match oxicast::discovery::discover_devices(config.discovery_timeout).await {
            Ok(found) => devices.extend(found.into_iter().map(from_info)),
            Err(err) => {
                debug!(error = %err, "Chromecast mDNS scan failed");
                if config.known_cast_hosts.is_empty() {
                    return Err(err.to_string());
                }
            }
        }

        for known in &config.known_cast_hosts {
            if devices.iter().any(|d| d.device.address == known.host) {
                continue;
            }
            devices.push(from_known(&known.host, known.port));
        }

        Ok(devices)
    }
}

fn from_info(info: DeviceInfo) -> RawDevice {
    let uuid = info.uuid.clone();
    let id = match &uuid {
        Some(value) if !value.is_empty() => format!("cast-{value}"),
        _ => format!(
            "cast-{}",
            stable_id(&[&info.ip.to_string(), &info.port.to_string()])
        ),
    };
    let model = info.model.clone().unwrap_or_else(|| "Chromecast".into());
    RawDevice {
        device: Device {
            id,
            name: info.name,
            r#type: DeviceType::Chromecast,
            manufacturer: "Google".into(),
            model,
            address: info.ip.to_string(),
            port: info.port,
            capabilities: Capabilities::chromecast(),
            location: None,
            ready: true,
        },
        uuid,
        dlna_location: None,
        cast_port: Some(info.port),
    }
}

fn from_known(host: &str, port: u16) -> RawDevice {
    RawDevice {
        device: Device {
            id: format!("cast-{}", stable_id(&[host, &port.to_string()])),
            name: host.to_string(),
            r#type: DeviceType::Chromecast,
            manufacturer: "Google".into(),
            model: "Chromecast".into(),
            address: host.to_string(),
            port,
            capabilities: Capabilities::chromecast(),
            location: None,
            ready: true,
        },
        uuid: None,
        dlna_location: None,
        cast_port: Some(port),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn normalizes_cast_device() {
        let raw = from_info(DeviceInfo {
            name: "Living Room TV".into(),
            ip: Ipv4Addr::new(192, 168, 1, 50).into(),
            port: 8009,
            model: Some("Chromecast Ultra".into()),
            uuid: Some("abcd".into()),
        });
        assert_eq!(raw.device.id, "cast-abcd");
        assert_eq!(raw.device.r#type, DeviceType::Chromecast);
        assert!(raw.device.capabilities.play);
        assert_eq!(raw.device.address, "192.168.1.50");
    }
}

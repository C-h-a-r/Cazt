use crate::models::{Capabilities, Device, DeviceType};

/// Protocol-level device before UI normalization/dedup.
#[derive(Debug, Clone)]
pub struct RawDevice {
    pub device: Device,
    pub uuid: Option<String>,
    pub dlna_location: Option<String>,
    pub cast_port: Option<u16>,
}

/// Merge devices discovered through multiple mechanisms.
///
/// Chromecast and DLNA hits on the same IP are treated as one physical
/// screen. Chromecast is preferred for control because it has a more
/// reliable playback-status channel.
pub fn deduplicate(devices: Vec<RawDevice>) -> Vec<RawDevice> {
    let mut merged: Vec<RawDevice> = Vec::new();

    for incoming in devices {
        if let Some(existing) = merged
            .iter_mut()
            .find(|candidate| same_physical(candidate, &incoming))
        {
            *existing = merge_pair(existing, &incoming);
        } else {
            merged.push(incoming);
        }
    }

    merged.sort_by(|a, b| {
        a.device
            .name
            .to_lowercase()
            .cmp(&b.device.name.to_lowercase())
    });
    merged
}

pub fn same_physical(a: &RawDevice, b: &RawDevice) -> bool {
    if let (Some(left), Some(right)) = (a.uuid.as_deref(), b.uuid.as_deref()) {
        if !left.is_empty() && left.eq_ignore_ascii_case(right) {
            return true;
        }
    }

    if a.device.address == b.device.address && !a.device.address.is_empty() {
        return true;
    }

    false
}

fn merge_pair(a: &RawDevice, b: &RawDevice) -> RawDevice {
    let prefer_cast = matches!(a.device.r#type, DeviceType::Chromecast)
        || matches!(b.device.r#type, DeviceType::Chromecast);

    let primary = if prefer_cast {
        if a.device.r#type == DeviceType::Chromecast {
            a
        } else {
            b
        }
    } else {
        a
    };
    let secondary = if std::ptr::eq(primary, a) { b } else { a };

    let mut device = primary.device.clone();
    device.name = nicer_name(&primary.device.name, &secondary.device.name);
    if device.manufacturer == "Unknown" {
        device.manufacturer = secondary.device.manufacturer.clone();
    }
    if device.model == "Unknown" {
        device.model = secondary.device.model.clone();
    }
    device.capabilities =
        Capabilities::merge(primary.device.capabilities, secondary.device.capabilities);
    device.ready = primary.device.ready || secondary.device.ready;
    if device.location.is_none() {
        device.location = secondary.device.location.clone();
    }

    RawDevice {
        device,
        uuid: primary.uuid.clone().or_else(|| secondary.uuid.clone()),
        dlna_location: primary
            .dlna_location
            .clone()
            .or_else(|| secondary.dlna_location.clone()),
        cast_port: primary.cast_port.or(secondary.cast_port),
    }
}

fn nicer_name(a: &str, b: &str) -> String {
    const GENERIC: &[&str] = &[
        "chromecast",
        "google tv",
        "android tv",
        "media renderer",
        "tv",
        "dlna",
    ];
    let score = |name: &str| {
        let lower = name.to_ascii_lowercase();
        let generic = GENERIC.iter().any(|g| lower == *g);
        (if generic { 0 } else { 10 }) + name.len()
    };
    if score(b) > score(a) {
        b.to_string()
    } else {
        a.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DeviceType;

    fn raw(id: &str, name: &str, address: &str, kind: DeviceType, uuid: Option<&str>) -> RawDevice {
        RawDevice {
            device: Device {
                id: id.into(),
                name: name.into(),
                r#type: kind,
                manufacturer: "Unknown".into(),
                model: "Unknown".into(),
                address: address.into(),
                port: 8009,
                capabilities: Capabilities::chromecast(),
                location: None,
                ready: true,
            },
            uuid: uuid.map(ToOwned::to_owned),
            dlna_location: None,
            cast_port: Some(8009),
        }
    }

    #[test]
    fn merges_cast_and_dlna_on_same_ip() {
        let devices = vec![
            raw(
                "c1",
                "Chromecast",
                "192.168.1.50",
                DeviceType::Chromecast,
                Some("abc"),
            ),
            raw(
                "d1",
                "Living Room TV",
                "192.168.1.50",
                DeviceType::Dlna,
                None,
            ),
        ];
        let merged = deduplicate(devices);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].device.r#type, DeviceType::Chromecast);
        assert_eq!(merged[0].device.name, "Living Room TV");
    }

    #[test]
    fn keeps_devices_on_different_ips() {
        let devices = vec![
            raw(
                "c1",
                "Living Room",
                "192.168.1.50",
                DeviceType::Chromecast,
                Some("a"),
            ),
            raw(
                "c2",
                "Bedroom",
                "192.168.1.51",
                DeviceType::Chromecast,
                Some("b"),
            ),
        ];
        let merged = deduplicate(devices);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merges_by_uuid_even_if_ip_differs() {
        let devices = vec![
            raw(
                "c1",
                "TV",
                "192.168.1.50",
                DeviceType::Chromecast,
                Some("same-id"),
            ),
            raw(
                "c2",
                "TV",
                "192.168.1.80",
                DeviceType::Chromecast,
                Some("same-id"),
            ),
        ];
        let merged = deduplicate(devices);
        assert_eq!(merged.len(), 1);
    }
}

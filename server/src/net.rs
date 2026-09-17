use std::net::IpAddr;

use crate::config::Config;

/// Best-effort LAN address that TVs can reach. Never returns loopback when a
/// real interface address exists.
pub fn advertised_host(config: &Config) -> Option<String> {
    if let Some(host) = &config.public_host {
        let host = host.trim();
        if !host.is_empty() && host != "localhost" && host != "127.0.0.1" && host != "::1" {
            return Some(host.to_string());
        }
    }

    if let Ok(ip) = local_ip_address::local_ip() {
        if is_advertisable(ip) {
            return Some(ip.to_string());
        }
    }

    if_addrs::get_if_addrs()
        .ok()?
        .into_iter()
        .filter(|iface| !iface.is_loopback())
        .map(|iface| iface.ip())
        .find(|ip| is_advertisable(*ip))
        .map(|ip| ip.to_string())
}

pub fn is_advertisable(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !v4.is_loopback()
                && !v4.is_unspecified()
                && !v4.is_link_local()
                && !v4.is_multicast()
                && !v4.is_broadcast()
        }
        IpAddr::V6(v6) => {
            !v6.is_loopback()
                && !v6.is_unspecified()
                && !v6.is_multicast()
                && !v6.is_unicast_link_local()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn loopback_is_not_advertisable() {
        assert!(!is_advertisable(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(!is_advertisable(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
    }

    #[test]
    fn private_lan_is_advertisable() {
        assert!(is_advertisable(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))));
        assert!(is_advertisable(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5))));
    }
}

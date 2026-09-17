use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use reqwest::redirect::Policy;
use url::Url;

use crate::error::{AppError, ErrorCode};

/// Guardrail for outbound fetches: only public http(s) URLs, no metadata/link-local
/// targets, and no redirects onto those networks.
pub fn assert_public_http_url(raw: &str) -> Result<Url, AppError> {
    let url = Url::parse(raw).map_err(|_| {
        AppError::bad_request(ErrorCode::InvalidInput, "That media URL is not valid.")
    })?;
    validate_url(&url)?;
    Ok(url)
}

pub fn validate_url(url: &Url) -> Result<(), AppError> {
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(AppError::forbidden(
            ErrorCode::SsrfBlocked,
            "Only http and https media URLs are allowed.",
        ));
    }
    let host = url.host_str().ok_or_else(|| {
        AppError::forbidden(ErrorCode::SsrfBlocked, "Media URL is missing a host.")
    })?;
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        return Err(blocked());
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(blocked());
        }
    }
    Ok(())
}

pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_v4(v4),
        IpAddr::V6(v6) => is_blocked_v6(v6),
    }
}

fn is_blocked_v4(ip: Ipv4Addr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_private()
        || ip.octets()[0] == 0
        || ip.octets()[0] == 100 && (ip.octets()[1] & 0b1100_0000) == 64 // 100.64/10
        || ip.octets()[0] == 192 && ip.octets()[1] == 0 && ip.octets()[2] == 2
        || ip.octets()[0] == 198 && ip.octets()[1] == 51 && ip.octets()[2] == 100
        || ip.octets()[0] == 203 && ip.octets()[1] == 0 && ip.octets()[2] == 113
}

fn is_blocked_v6(ip: Ipv6Addr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_unique_local()
        || ip.is_unicast_link_local()
        || ip.to_ipv4_mapped().map(is_blocked_v4).unwrap_or(false)
}

pub fn blocked() -> AppError {
    AppError::forbidden(
        ErrorCode::SsrfBlocked,
        "That address is not allowed. Cazt will not fetch private, loopback, or metadata URLs.",
    )
}

pub fn redirect_policy() -> Policy {
    Policy::custom(|attempt| {
        if attempt.previous().len() > 5 {
            return attempt.error(AppError::forbidden(
                ErrorCode::SsrfBlocked,
                "Too many redirects while fetching media.",
            ));
        }
        match validate_url(attempt.url()) {
            Ok(()) => attempt.follow(),
            Err(err) => attempt.error(err),
        }
    })
}

pub async fn resolve_host_blocked(host: &str) -> Result<(), AppError> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(blocked());
        }
        return Ok(());
    }
    let lookup = tokio::time::timeout(Duration::from_secs(5), tokio::net::lookup_host((host, 0)))
        .await
        .map_err(|_| AppError::timeout(ErrorCode::NetworkUnreachable, "DNS lookup timed out."))?
        .map_err(|e| {
            AppError::unavailable(
                ErrorCode::NetworkUnreachable,
                format!("Could not resolve host: {e}"),
            )
        })?;
    for addr in lookup {
        if is_blocked_ip(addr.ip()) {
            return Err(blocked());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_loopback_and_private() {
        assert!(assert_public_http_url("http://127.0.0.1/secret").is_err());
        assert!(assert_public_http_url("http://localhost/x").is_err());
        assert!(assert_public_http_url("http://192.168.1.1/x").is_err());
        assert!(assert_public_http_url("http://10.0.0.5/x").is_err());
        assert!(assert_public_http_url("http://169.254.169.254/latest").is_err());
        assert!(assert_public_http_url("file:///etc/passwd").is_err());
        assert!(assert_public_http_url("gopher://example.com").is_err());
    }

    #[test]
    fn allows_public_https() {
        assert!(
            assert_public_http_url("https://commondatastorage.googleapis.com/video.mp4").is_ok()
        );
        assert!(
            assert_public_http_url("https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8").is_ok()
        );
    }
}

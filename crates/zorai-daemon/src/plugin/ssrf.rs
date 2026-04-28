//! SSRF (Server-Side Request Forgery) validation for plugin API proxy.
//!
//! Blocks requests to internal/private IP ranges before any HTTP request is made.
//! Handles IPv4, IPv6, IPv4-mapped IPv6, link-local, cloud metadata, and ULA addresses.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::api_proxy::PluginApiError;

/// Check if an IP address belongs to a blocked (internal/private) range.
///
/// Blocked ranges:
/// - Loopback (127.x.x.x, ::1)
/// - Private (10.x, 172.16-31.x, 192.168.x)
/// - Link-local (169.254.x.x, fe80::/10)
/// - Cloud metadata (169.254.169.254)
/// - IPv6 ULA (fd00::/8)
/// - IPv4-mapped IPv6 (::ffff:127.0.0.1 etc.)
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_ipv4(&v4),
        IpAddr::V6(v6) => {
            // Check IPv4-mapped IPv6 first (e.g. ::ffff:127.0.0.1)
            if is_ipv4_mapped_blocked(&v6) {
                return true;
            }
            is_blocked_ipv6(&v6)
        }
    }
}

/// Check if an IPv4 address is in a blocked range.
fn is_blocked_ipv4(v4: &Ipv4Addr) -> bool {
    v4.is_loopback() || v4.is_private() || v4.is_link_local() || is_cloud_metadata_v4(v4)
}

/// Check for cloud metadata endpoint (169.254.169.254).
fn is_cloud_metadata_v4(v4: &Ipv4Addr) -> bool {
    v4.octets() == [169, 254, 169, 254]
}

/// Check if an IPv6 address is in a blocked range.
fn is_blocked_ipv6(v6: &Ipv6Addr) -> bool {
    v6.is_loopback() || is_ipv6_ula(v6) || is_ipv6_link_local(v6)
}

/// Check if an IPv6 address is a Unique Local Address (fd00::/8).
fn is_ipv6_ula(v6: &Ipv6Addr) -> bool {
    v6.segments()[0] & 0xff00 == 0xfd00
}

/// Check if an IPv6 address is link-local (fe80::/10).
fn is_ipv6_link_local(v6: &Ipv6Addr) -> bool {
    v6.segments()[0] & 0xffc0 == 0xfe80
}

/// Check if an IPv6 address is an IPv4-mapped address that maps to a blocked IPv4.
fn is_ipv4_mapped_blocked(v6: &Ipv6Addr) -> bool {
    if let Some(v4) = v6.to_ipv4_mapped() {
        is_blocked_ipv4(&v4)
    } else {
        false
    }
}

/// Validate a URL for SSRF safety by resolving DNS and checking all resolved IPs.
///
/// If `allow_local` is true, the SSRF check is bypassed entirely (for dev/testing).
pub async fn validate_url(url: &str, allow_local: bool) -> Result<(), PluginApiError> {
    if allow_local {
        return Ok(());
    }

    // Parse the URL to extract host and port
    let parsed = url::Url::parse(url).map_err(|e| PluginApiError::SsrfBlocked {
        url: format!("{url} (invalid URL: {e})"),
    })?;

    let host = parsed
        .host_str()
        .ok_or_else(|| PluginApiError::SsrfBlocked {
            url: format!("{url} (no host)"),
        })?;

    let port = parsed.port_or_known_default().unwrap_or(443);
    let addr_str = format!("{host}:{port}");

    // Resolve DNS and check all addresses
    let addrs =
        tokio::net::lookup_host(&addr_str)
            .await
            .map_err(|e| PluginApiError::SsrfBlocked {
                url: format!("{url} (DNS resolution failed: {e})"),
            })?;

    for addr in addrs {
        if is_blocked_ip(addr.ip()) {
            return Err(PluginApiError::SsrfBlocked {
                url: url.to_string(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    // -----------------------------------------------------------------------
    // IPv4 blocked ranges
    // -----------------------------------------------------------------------

    #[test]
    fn blocks_loopback() {
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
    }

    #[test]
    fn blocks_private_10() {
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
    }

    #[test]
    fn blocks_private_172_16() {
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
    }

    #[test]
    fn blocks_private_192_168() {
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn blocks_cloud_metadata() {
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254))));
    }

    #[test]
    fn blocks_link_local() {
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))));
    }

    #[test]
    fn allows_public_ipv4() {
        assert!(!is_blocked_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }

    // -----------------------------------------------------------------------
    // IPv6 blocked ranges
    // -----------------------------------------------------------------------

    #[test]
    fn blocks_ipv6_loopback() {
        assert!(is_blocked_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn blocks_ipv4_mapped_ipv6_loopback() {
        // ::ffff:127.0.0.1
        let v6 = Ipv4Addr::new(127, 0, 0, 1).to_ipv6_mapped();
        assert!(is_blocked_ip(IpAddr::V6(v6)));
    }

    #[test]
    fn blocks_ipv6_ula() {
        let v6: Ipv6Addr = "fd00::1".parse().unwrap();
        assert!(is_blocked_ip(IpAddr::V6(v6)));
    }

    #[test]
    fn allows_public_ipv6() {
        // Google public DNS IPv6
        let v6: Ipv6Addr = "2001:4860:4860::8888".parse().unwrap();
        assert!(!is_blocked_ip(IpAddr::V6(v6)));
    }

    // -----------------------------------------------------------------------
    // allow_local bypass
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn allow_local_bypasses_ssrf_check() {
        // This would normally fail if DNS resolved to 127.0.0.1,
        // but allow_local=true skips the check entirely.
        let result = validate_url("http://localhost:8080/api", true).await;
        assert!(result.is_ok());
    }
}

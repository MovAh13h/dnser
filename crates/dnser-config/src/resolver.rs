use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use serde::Deserialize;

/// Forwarding resolver configuration.
///
/// TOML section: `[resolver]`
///
/// ```toml
/// [resolver]
/// upstreams  = ["8.8.8.8:53", "1.1.1.1:53"]
/// timeout_ms = 2000
/// ```
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ResolverConfig {
    /// Upstream nameservers queried concurrently; first success wins. Defaults to Google and Cloudflare.
    pub upstreams: Vec<SocketAddr>,
    /// Per-upstream query timeout in milliseconds. Defaults to 2000.
    pub timeout_ms: u64,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            upstreams: vec![
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 53),
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 53),
            ],
            timeout_ms: 2000,
        }
    }
}

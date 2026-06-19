use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use serde::Deserialize;

/// Default UDP/TCP port the server listens on.
pub const DEFAULT_PORT: u16 = 1053;

/// Network and threading configuration for the DNS server.
///
/// TOML section: `[server]`
///
/// ```toml
/// [server]
/// listen  = "0.0.0.0:1053"
/// workers = 4
/// ```
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Socket address the server binds to.
    ///
    /// Defaults to `0.0.0.0:1053`.
    pub listen: SocketAddr,

    /// Number of worker threads. Defaults to `1`.
    pub workers: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_PORT),
            workers: 1,
        }
    }
}

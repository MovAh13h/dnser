use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use serde::Deserialize;

/// Default UDP/TCP port the server listens on when no `listen` addresses are configured.
pub const DEFAULT_PORT: u16 = 1053;

/// Network and threading configuration for the DNS server.
///
/// TOML section: `[server]`
///
/// ```toml
/// [server]
/// listen  = ["0.0.0.0:1053", "[::]:1053"]
/// workers = 4
/// ```
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Socket addresses the server binds to.
    ///
    /// Defaults to `["0.0.0.0:1053"]`. Must contain at least one entry.
    pub listen: Vec<SocketAddr>,

    /// Number of worker threads.
    ///
    /// `None` (the default) defers to [`std::thread::available_parallelism`].
    /// Must be `>= 1` when set explicitly.
    pub workers: Option<usize>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: vec![SocketAddr::new(
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                DEFAULT_PORT,
            )],
            workers: None,
        }
    }
}

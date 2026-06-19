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
/// listen               = "0.0.0.0:1053"
/// workers              = 4
/// tokio_threads        = 4
/// shutdown_drain_secs  = 5
/// ```
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Socket address the server binds to. Defaults to `0.0.0.0:1053`.
    pub listen: SocketAddr,

    /// Number of worker tasks. Each binds its own `SO_REUSEPORT` socket so the
    /// kernel distributes incoming UDP datagrams across them. Defaults to `1`.
    pub workers: usize,

    /// Number of tokio runtime threads. `0` means use tokio's default (one per
    /// logical CPU). Set equal to `workers` when you want one dedicated thread
    /// per worker; leave at `0` to let tokio decide. Defaults to `0`.
    pub tokio_threads: usize,

    /// Seconds to wait for in-flight queries to finish after a shutdown signal
    /// before forcefully aborting them. Defaults to `5`.
    pub shutdown_drain_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_PORT),
            workers: 1,
            tokio_threads: 0,
            shutdown_drain_secs: 5,
        }
    }
}

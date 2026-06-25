use serde::Deserialize;

/// Cache configuration.
///
/// TOML section: `[cache]`
///
/// ```toml
/// [cache]
/// max_entries           = 10_000
/// reaper_interval_secs  = 30
/// max_negative_ttl_secs = 3600
/// ```
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    /// Maximum number of cached DNS responses. When full, one entry is evicted to make room.
    /// Defaults to `10_000`.
    pub max_entries: usize,

    /// How often the background reaper task sweeps for expired entries, in seconds.
    /// Defaults to `30`.
    pub reaper_interval_secs: u64,

    /// Maximum TTL honored for negative (NXDOMAIN / NODATA) cache entries, in seconds.
    /// Upstream SOA minimums exceeding this value are clamped. Defaults to `3600` (RFC 2308 §5).
    pub max_negative_ttl_secs: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 10_000,
            reaper_interval_secs: 30,
            max_negative_ttl_secs: 3600,
        }
    }
}

use std::fmt;

/// Errors that can occur while loading or validating configuration.
#[derive(Debug)]
pub enum ConfigError {
    /// The config file could not be read.
    ///
    /// Contains the file path (for display) and the underlying [`std::io::Error`].
    Io(String, std::io::Error),

    /// The config file contained invalid TOML or an unrecognised field.
    Parse(toml::de::Error),

    /// `[cache] max_entries` was set to zero; the cache would never store anything.
    ZeroCacheCapacity,

    /// `[cache] reaper_interval_secs` was set to zero; the reaper would spin at 100% CPU.
    ZeroReaperInterval,

    /// `[server] workers` was set to zero; no UDP sockets would be bound.
    ZeroWorkers,

    /// `[server] tcp_idle_timeout_secs` was set to zero.
    ZeroTcpIdleTimeout,

    /// `[server] tcp_max_connections` was set to zero.
    ZeroTcpMaxConnections,

    /// `[cache] max_negative_ttl_secs` was set to zero.
    ZeroMaxNegativeTtl,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, e) => write!(f, "cannot read config file {path}: {e}"),
            Self::Parse(e) => write!(f, "failed to parse config: {e}"),
            Self::ZeroCacheCapacity => write!(f, "[cache] max_entries must be >= 1"),
            Self::ZeroReaperInterval => write!(f, "[cache] reaper_interval_secs must be >= 1"),
            Self::ZeroWorkers => write!(f, "[server] workers must be >= 1"),
            Self::ZeroTcpIdleTimeout => {
                write!(f, "[server] tcp_idle_timeout_secs must be >= 1")
            }
            Self::ZeroTcpMaxConnections => {
                write!(f, "[server] tcp_max_connections must be >= 1")
            }
            Self::ZeroMaxNegativeTtl => {
                write!(f, "[cache] max_negative_ttl_secs must be >= 1")
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(_, e) => Some(e),
            Self::Parse(e) => Some(e),
            Self::ZeroCacheCapacity
            | Self::ZeroReaperInterval
            | Self::ZeroWorkers
            | Self::ZeroTcpIdleTimeout
            | Self::ZeroTcpMaxConnections
            | Self::ZeroMaxNegativeTtl => None,
        }
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        Self::Parse(e)
    }
}

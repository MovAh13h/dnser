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

    /// `server.listen` was set to an empty list.
    ///
    /// The server cannot start without at least one bind address.
    EmptyListen,

    /// `server.workers` was explicitly set to `0`.
    ///
    /// Use `None` to let the runtime choose, or set a value `>= 1`.
    ZeroWorkers,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, e) => write!(f, "cannot read config file {path}: {e}"),
            Self::Parse(e) => write!(f, "failed to parse config: {e}"),
            Self::EmptyListen => write!(f, "server.listen must not be empty"),
            Self::ZeroWorkers => write!(f, "server.workers must be greater than 0"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(_, e) => Some(e),
            Self::Parse(e) => Some(e),
            _ => None,
        }
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        Self::Parse(e)
    }
}

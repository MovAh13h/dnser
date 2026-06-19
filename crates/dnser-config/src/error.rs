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
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, e) => write!(f, "cannot read config file {path}: {e}"),
            Self::Parse(e) => write!(f, "failed to parse config: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(_, e) => Some(e),
            Self::Parse(e) => Some(e),
        }
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        Self::Parse(e)
    }
}

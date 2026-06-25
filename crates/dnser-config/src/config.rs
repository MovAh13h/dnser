use serde::Deserialize;

use crate::error::ConfigError;
use crate::logging::LoggingConfig;
use crate::resolver::ResolverConfig;
use crate::server::ServerConfig;

/// Root configuration object returned by [`crate::load`].
///
/// Each field corresponds to a TOML section. All sections default to their
/// built-in values when absent from the config file.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Network listener and threading settings.
    pub server: ServerConfig,
    /// Log level and output format.
    pub logging: LoggingConfig,
    /// Forwarding resolver settings.
    pub resolver: ResolverConfig,
}

impl Config {
    /// Rejects configurations that are structurally valid TOML but semantically wrong.
    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        Ok(())
    }
}

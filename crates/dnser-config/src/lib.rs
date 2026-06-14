//! Configuration loading for the `dnser` DNS server.
//!
//! Call [`load`] once at startup, then pass typed references to subsystems:
//!
//! ```no_run
//! use dnser_config::load;
//!
//! let config = load(None).expect("invalid config");
//! // pass &config.server to the server subsystem, &config.logging to the logger, etc.
//! ```
//!
//! The config file is TOML. All sections and fields are optional; omitting them
//! yields the built-in defaults.

mod config;
mod error;
mod logging;
mod server;

pub use config::Config;
pub use error::ConfigError;
pub use logging::{LogFormat, LogLevel, LoggingConfig};
pub use server::{DEFAULT_PORT, ServerConfig};

use std::path::Path;

/// Loads config from `path` (if given), falling back to built-in defaults.
///
/// Reads and parses `path` as TOML when provided. Passes `None` to skip file
/// I/O and start from defaults — useful in tests or when no config file is
/// expected.
///
/// # Errors
///
/// Returns [`ConfigError`] if:
/// - the file cannot be read ([`ConfigError::Io`])
/// - the TOML is malformed ([`ConfigError::Parse`])
/// - a value is semantically invalid ([`ConfigError::EmptyListen`], [`ConfigError::ZeroWorkers`])
pub fn load(path: Option<&Path>) -> Result<Config, ConfigError> {
    let config: Config = match path {
        Some(p) => {
            let s = std::fs::read_to_string(p)
                .map_err(|e| ConfigError::Io(p.display().to_string(), e))?;
            toml::from_str(&s)?
        }
        None => Config::default(),
    };
    config.validate()?;
    Ok(config)
}

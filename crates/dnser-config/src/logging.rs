use serde::Deserialize;

/// Logging configuration.
///
/// TOML section: `[logging]`
///
/// ```toml
/// [logging]
/// level  = "debug"
/// format = "json"
/// ```
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Minimum severity to emit. Defaults to [`LogLevel::Info`].
    pub level: LogLevel,
    /// Output format. Defaults to [`LogFormat::Pretty`].
    pub format: LogFormat,
}

/// Log severity filter.
#[derive(Debug, Default, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

/// Log output format.
#[derive(Debug, Default, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Human-readable, colorized output for development.
    #[default]
    Pretty,
    /// Structured JSON output for log aggregation pipelines.
    Json,
}

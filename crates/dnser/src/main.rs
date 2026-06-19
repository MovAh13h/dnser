use dnser_config::{LogFormat, LogLevel, LoggingConfig};
use tracing::Level;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let config = dnser_config::load(None)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    init_logging(&config.logging);
    dnser_server::run(config.server).await
}

fn init_logging(config: &LoggingConfig) {
    let level = match config.level {
        LogLevel::Trace => Level::TRACE,
        LogLevel::Debug => Level::DEBUG,
        LogLevel::Info => Level::INFO,
        LogLevel::Warn => Level::WARN,
        LogLevel::Error => Level::ERROR,
    };

    match config.format {
        LogFormat::Pretty => tracing_subscriber::fmt().with_max_level(level).init(),
        LogFormat::Json => tracing_subscriber::fmt()
            .json()
            .with_max_level(level)
            .init(),
    }
}

use dnser_config::{LogFormat, LogLevel, LoggingConfig};
use tracing::Level;

fn main() -> Result<(), std::io::Error> {
    let config = dnser_config::load(None)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    init_logging(&config.logging);

    let mut builder = tokio::runtime::Builder::new_multi_thread();
    if config.server.tokio_threads > 0 {
        builder.worker_threads(config.server.tokio_threads);
    }

    builder
        .enable_all()
        .build()?
        .block_on(dnser_server::run(config.server))
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

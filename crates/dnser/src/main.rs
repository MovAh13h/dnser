use std::path::PathBuf;
use std::process;

use dnser_config::{LogFormat, LogLevel, LoggingConfig};
use tracing::Level;

fn main() -> Result<(), std::io::Error> {
    let config_path = parse_args();

    let config = dnser_config::load(config_path.as_deref())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    init_logging(&config.logging);

    let mut builder = tokio::runtime::Builder::new_multi_thread();
    if config.server.tokio_threads > 0 {
        builder.worker_threads(config.server.tokio_threads);
    }

    builder
        .enable_all()
        .build()?
        .block_on(dnser_server::run(config))
}

fn parse_args() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    let mut config_path: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" | "-c" => {
                let path = args.next().unwrap_or_else(|| {
                    eprintln!("error: --config requires a path argument");
                    process::exit(1);
                });
                config_path = Some(PathBuf::from(path));
            }
            "--help" | "-h" => {
                println!(
                    "Usage: dnser [OPTIONS]\n\nOptions:\n  -c, --config <FILE>  Path to TOML config file (default: built-in defaults)\n  -V, --version        Print version\n  -h, --help           Print this help"
                );
                process::exit(0);
            }
            "--version" | "-V" => {
                println!("dnser {}", env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            other => {
                eprintln!("error: unknown argument '{other}'\nRun 'dnser --help' for usage.");
                process::exit(1);
            }
        }
    }

    config_path
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

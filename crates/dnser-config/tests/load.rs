use std::io::Write;
use std::net::SocketAddr;
use std::path::Path;

use dnser_config::{ConfigError, DEFAULT_PORT, LogFormat, LogLevel, load};
use tempfile::NamedTempFile;

fn config_file(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    write!(f, "{content}").unwrap();
    f
}

fn default_listen() -> Vec<SocketAddr> {
    vec![format!("0.0.0.0:{DEFAULT_PORT}").parse().unwrap()]
}

#[test]
fn defaults_when_no_path() {
    let config = load(None).unwrap();
    assert_eq!(config.server.listen, default_listen());
    assert_eq!(config.server.workers, None);
    assert_eq!(config.logging.level, LogLevel::Info);
    assert_eq!(config.logging.format, LogFormat::Pretty);
}

#[test]
fn parses_server_section() {
    let f = config_file(
        r#"
        [server]
        listen = ["127.0.0.1:5353"]
        workers = 4
        "#,
    );
    let config = load(Some(f.path())).unwrap();
    assert_eq!(
        config.server.listen,
        vec!["127.0.0.1:5353".parse().unwrap()]
    );
    assert_eq!(config.server.workers, Some(4));
}

#[test]
fn parses_logging_section() {
    let f = config_file("[logging]\nlevel = \"debug\"\nformat = \"json\"");
    let config = load(Some(f.path())).unwrap();
    assert_eq!(config.logging.level, LogLevel::Debug);
    assert_eq!(config.logging.format, LogFormat::Json);
}

#[test]
fn empty_listen_is_rejected() {
    let f = config_file("[server]\nlisten = []");
    assert!(matches!(
        load(Some(f.path())),
        Err(ConfigError::EmptyListen)
    ));
}

#[test]
fn zero_workers_is_rejected() {
    let f = config_file("[server]\nworkers = 0");
    assert!(matches!(
        load(Some(f.path())),
        Err(ConfigError::ZeroWorkers)
    ));
}

#[test]
fn missing_file_is_an_error() {
    let result = load(Some(Path::new("/nonexistent/path/config.toml")));
    assert!(matches!(result, Err(ConfigError::Io(_, _))));
}

#[test]
fn malformed_toml_is_an_error() {
    let f = config_file("this is not [ valid toml");
    assert!(matches!(load(Some(f.path())), Err(ConfigError::Parse(_))));
}

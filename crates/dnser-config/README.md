# dnser-config

Configuration loading for [dnser](https://github.com/MovAh13h/dnser). Reads a TOML file at startup, falls back to built-in defaults when no file is given, and exposes strongly-typed, read-only structs to the rest of the application.

## Usage

```rust
use std::path::Path;

// With a config file
let config = dnser_config::load(Some(Path::new("/etc/dnser/config.toml")))?;

// Defaults only (useful in tests or development)
let config = dnser_config::load(None)?;

// Pass typed references to subsystems, not the full Config
start_server(&config.server);
```

## Config file

The config file is TOML. Every field is optional — omitting a section or field falls back to its built-in default.

```toml
[server]
listen  = ["0.0.0.0:1053"]
workers = 4

[logging]
level  = "info"
format = "pretty"
```

## Errors

`load()` returns a `ConfigError` in three situations:

- The file path was given but cannot be read (`ConfigError::Io`)
- The file is not valid TOML or a field has the wrong type (`ConfigError::Parse`)
- A value is structurally valid but semantically wrong, e.g. an empty listener list (`ConfigError::EmptyListen`, `ConfigError::ZeroWorkers`)

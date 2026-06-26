# dnser-config

TOML configuration loader for [dnser](../../). Reads a config file at startup (or skips file I/O entirely with `None`), validates it, and exposes strongly-typed, read-only structs to the rest of the application.

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

The file is TOML. Every section and every field is optional — anything you omit falls back to its built-in default. See the [root README](../../README.md) for the complete reference; a minimal example:

```toml
[server]
listen  = "0.0.0.0:1053"
workers = 4

[logging]
level  = "info"
format = "pretty"
```

## Errors

`load()` returns a `ConfigError` for:

- File I/O failure (`ConfigError::Io`)
- Malformed TOML or wrong field types (`ConfigError::Parse`)
- Semantically invalid values, e.g. zero workers / zero capacity (`ConfigError::ZeroWorkers`, `ConfigError::ZeroCacheCapacity`, etc.)

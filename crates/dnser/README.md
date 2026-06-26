# dnser

The `dnser` CLI binary. Loads configuration, initializes logging and the tokio runtime, and runs [`dnser-server`](../dnser-server/) until a shutdown signal arrives.

## Usage

```bash
dnser [OPTIONS]

Options:
  -c, --config <FILE>  Path to TOML config file (default: built-in defaults)
  -V, --version        Print version
  -h, --help           Print this help
```

Example:

```bash
dnser --config /etc/dnser/config.toml
```

With no `--config`, `dnser` runs with built-in defaults (binds `0.0.0.0:1053`, forwards to `8.8.8.8` and `1.1.1.1`). See the [root README](../../README.md) for the full configuration reference.

`Ctrl-C` or `SIGTERM` triggers a graceful drain (bounded by `shutdown_drain_secs`) before exit.

## Privileged ports

Binding port 53 needs root or `CAP_NET_BIND_SERVICE`:

```bash
sudo setcap 'cap_net_bind_service=+ep' $(which dnser)
```

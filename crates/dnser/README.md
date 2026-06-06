# dnser

The main binary crate for the dnser DNS server. It wires together the server runtime, configuration loader, and logging setup, and exposes a CLI for controlling bind address, config file path, and other runtime options.

## Usage

Build and run:

```bash
cargo build --release -p dnser
./target/release/dnser
```

Query the server (default port 1053):

```bash
dig @127.0.0.1 -p 1053 example.com
```

For production on port 53, either run as root or grant `CAP_NET_BIND_SERVICE` on Linux.

Shut down gracefully with `Ctrl+C` or `SIGTERM`.

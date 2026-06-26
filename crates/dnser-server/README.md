# dnser-server

Async UDP + TCP server runtime for [dnser](../../). Owns the socket listeners, dispatches incoming queries to the resolver, and writes responses back.

## What it does

- N UDP workers, each with its own `SO_REUSEPORT` socket on the configured port, so the kernel sprays datagrams across them without userspace contention.
- One TCP listener accepting connections, with per-connection idle timeout (RFC 7766 §6.2.3) and a global cap on simultaneous connections.
- UDP truncation handling per RFC 1035 §4.2.1: if a reply would exceed the client's UDP size limit (EDNS(0) advertised, otherwise 512 bytes), a truncated reply with the `TC` bit is sent so the client retries over TCP.
- EDNS(0) OPT handling per RFC 6891 including `BADVERS` for unsupported versions.
- Global semaphore-backed backpressure: bounded in-flight UDP queries, bounded simultaneous TCP connections. Excess load is dropped at the edge rather than queued.
- Graceful shutdown via `watch` channel: stops accepting, drains in-flight queries within `shutdown_drain_secs`, then aborts the remainder.

## Usage

```rust
use dnser_config::Config;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let config = dnser_config::load(None)?;
    let handle = dnser_server::start(config).await?;
    println!("UDP {} / TCP {}", handle.udp_addr, handle.tcp_addr);

    // ... run until you want to stop ...

    handle.shutdown().await;
    Ok(())
}
```

`start()` returns immediately with a `ServerHandle`; the server is already accepting queries. `run()` is a convenience wrapper that calls `start()` then waits for `Ctrl-C` / `SIGTERM` before shutting down.

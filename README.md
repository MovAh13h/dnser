# dnser

[![CI](https://github.com/MovAh13h/dnser/actions/workflows/ci.yml/badge.svg)](https://github.com/MovAh13h/dnser/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A fast, RFC-compliant DNS forwarding resolver in Rust.

`dnser` listens for DNS queries over UDP and TCP, races them across the upstream resolvers you configure, and caches the answers — positive *and* negative — with the right TTL semantics. It is designed to be the kind of resolver you drop in front of a fleet of hosts, a container network, or your laptop, and forget about.

```
clients ──▶ dnser ──┬──▶ 1.1.1.1
                    └──▶ 8.8.8.8
                    (first valid reply wins)
```

## Highlights

- **UDP + TCP** listener with proper truncation (TC bit) handover per RFC 1035 §4.2.1
- **TCP framing** per RFC 1035 §4.2.2 with idle-timeout and connection cap (RFC 7766 §6.2.3)
- **Sharded concurrent cache** with per-entry TTL and background reaping
- **Negative caching** of NXDOMAIN / NODATA per RFC 2308, clamped by SOA `MINIMUM`
- **EDNS(0)** OPT handling per RFC 6891 — advertised UDP size, BADVERS for unsupported versions
- **Upstream racing** — every configured upstream is queried in parallel; the first valid reply wins
- **Random query IDs** for upstream queries per RFC 5452 §9.2
- **SO_REUSEPORT** worker pool — N kernel-balanced UDP sockets on the same port
- **Backpressure** — bounded in-flight UDP queries and bounded simultaneous TCP connections
- **Graceful shutdown** on SIGTERM / Ctrl-C with bounded drain
- **Structured logging** via `tracing` — pretty for humans, JSON for log pipelines

## Install

From source:

```bash
git clone https://github.com/MovAh13h/dnser.git
cd dnser
cargo install --path crates/dnser
```

Or run directly without installing:

```bash
cargo run --release -p dnser
```

`dnser` requires Rust 1.85+ (edition 2024).

## Run

With built-in defaults (binds `0.0.0.0:1053`, forwards to `8.8.8.8` and `1.1.1.1`):

```bash
dnser
```

With a config file:

```bash
dnser --config /etc/dnser/config.toml
```

Test it:

```bash
dig @127.0.0.1 -p 1053 example.com
```

Shut down with `Ctrl-C` or `SIGTERM` — `dnser` stops accepting new queries, waits for in-flight ones to drain (up to `shutdown_drain_secs`), then exits.

### Binding to port 53

DNS clients expect port 53. Binding it requires either root or, on Linux, the `CAP_NET_BIND_SERVICE` capability:

```bash
sudo setcap 'cap_net_bind_service=+ep' $(which dnser)
```

After that, set `listen = "0.0.0.0:53"` in your config and run as an unprivileged user.

## Configure

`dnser` is configured by a TOML file. Every section and every field is optional — anything you omit falls back to the built-in default.

```toml
[server]
listen                = "0.0.0.0:1053"   # bind address (UDP + TCP)
workers               = 4                # number of SO_REUSEPORT UDP workers
tokio_threads         = 0                # 0 = one per CPU
shutdown_drain_secs   = 5                # max seconds to drain in-flight on shutdown
tcp_idle_timeout_secs = 10               # close TCP connections after this much idle time
tcp_max_connections   = 1000             # cap on simultaneous TCP connections
udp_max_inflight      = 1000             # cap on in-flight UDP queries (across all workers)

[resolver]
upstreams  = ["1.1.1.1:53", "8.8.8.8:53"]  # raced concurrently; first valid reply wins
timeout_ms = 2000                           # per-upstream query timeout

[cache]
max_entries           = 10_000  # LRU-evicted when full
reaper_interval_secs  = 30      # background sweep cadence for expired entries
max_negative_ttl_secs = 3600    # clamp on RFC 2308 negative TTL

[logging]
level  = "info"    # trace | debug | info | warn | error
format = "pretty"  # pretty | json
```

The defaults above are the actual defaults — running `dnser` with no config is equivalent to writing this file out and pointing at it.

## How it works

A query enters the server over UDP or TCP, is parsed into a `Message`, and dispatched to the resolver. The resolver first checks the cache; on a hit, the cached answer is returned with TTLs decremented to reflect how long it has sat. On a miss, the resolver opens a fresh UDP socket per attempt, assigns a random 16-bit query ID (RFC 5452 §9.2), and sends the question in parallel to every configured upstream. The first parseable reply with matching ID and question wins; the rest are dropped. If the response would exceed the client's UDP size limit (EDNS(0) advertised, otherwise 512 bytes), `dnser` sends a truncated reply with the `TC` bit set so the client retries over TCP.

Positive answers are cached for their advertised TTL. Negative answers (NXDOMAIN, NODATA) are cached using the SOA `MINIMUM` field per RFC 2308 §5, clamped by `max_negative_ttl_secs`. A background reaper periodically sweeps expired entries; lookups also lazily skip anything past its expiry.

Each UDP worker holds its own `SO_REUSEPORT` socket on the configured port, so the kernel sprays datagrams across them without userspace contention. A single global semaphore caps total in-flight queries across all workers; excess datagrams are dropped on arrival rather than queued, providing backpressure against floods.

## Scope and non-goals

`dnser` is a **forwarding** resolver — it answers by relaying queries to upstreams you configure, not by iterating the DNS hierarchy itself. There is no root-hint following, no NS chasing, no CNAME unwinding beyond what upstreams return, and no authoritative-server support (it does not serve zone files).

DNSSEC validation is **not** performed in `dnser` itself; if you need DNSSEC, point `dnser` at a validating upstream and pass the `AD` bit through.

DoT (DNS-over-TLS) and DoH (DNS-over-HTTPS) are not yet implemented for either the listener or the upstream side.

## Library use

Although `dnser` is primarily a binary, the runtime is exposed as a library so you can embed it:

```rust
use dnser_config::Config;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let config = dnser_config::load(None)?;       // built-in defaults
    let handle = dnser_server::start(config).await?;
    println!("listening on UDP {} / TCP {}", handle.udp_addr, handle.tcp_addr);

    // ... do work ...

    handle.shutdown().await;
    Ok(())
}
```

`start()` returns immediately; the server is already accepting queries. `shutdown()` triggers a graceful drain.

## RFCs implemented

- **RFC 1035** — DNS wire format, UDP/TCP transport, truncation
- **RFC 2308** — Negative caching (NXDOMAIN, NODATA) using SOA `MINIMUM`
- **RFC 5452 §9.2** — Random source-port and 16-bit query ID for upstream queries
- **RFC 6891** — EDNS(0): OPT pseudo-RR, advertised UDP payload size, BADVERS
- **RFC 7766** — DNS-over-TCP: framing, idle timeout, connection limits

## Repository layout

The codebase is a Cargo workspace; the `dnser` binary depends on a set of focused library crates. Each has its own README:

- [`dnser`](crates/dnser/) — CLI binary
- [`dnser-server`](crates/dnser-server/) — async UDP + TCP server runtime
- [`dnser-resolver`](crates/dnser-resolver/) — forwarding resolver with upstream racing
- [`dnser-cache`](crates/dnser-cache/) — sharded TTL-aware cache with negative caching
- [`dnser-proto`](crates/dnser-proto/) — DNS wire format
- [`dnser-net`](crates/dnser-net/) — shared async transport helpers
- [`dnser-config`](crates/dnser-config/) — TOML config loader

## License

MIT — see [LICENSE](LICENSE).

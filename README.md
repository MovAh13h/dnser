# dnser

[![CI](https://github.com/MovAh13h/dnser/actions/workflows/ci.yml/badge.svg)](https://github.com/MovAh13h/dnser/actions/workflows/ci.yml)

> **Status: early development**

A DNS forwarding resolver written in Rust. Listens for DNS queries over UDP and TCP, races them against a configured set of upstream resolvers, and caches the answers with TTL- and NXDOMAIN-aware eviction.

## Run

```bash
cargo build --release -p dnser
./target/release/dnser
# default bind: 0.0.0.0:1053

dig @127.0.0.1 -p 1053 example.com
```

Listen address, upstreams, cache size, and worker count are configurable via TOML — see [`dnser-config`](crates/dnser-config/README.md).

## Workspace layout

| Crate | Role |
|---|---|
| [`dnser`](crates/dnser/README.md) | CLI binary; wires the other crates together |
| [`dnser-server`](crates/dnser-server/README.md) | Async UDP+TCP listener, query dispatch, UDP truncation (TC) handling |
| [`dnser-resolver`](crates/dnser-resolver/README.md) | Forwards queries upstream, races multiple upstreams, returns first valid reply |
| [`dnser-cache`](crates/dnser-cache/README.md) | TTL-aware concurrent record cache, with negative caching per RFC 2308 |
| [`dnser-proto`](crates/dnser-proto/README.md) | DNS wire format: header, question, RR, EDNS(0), per RFC 1035 |
| [`dnser-config`](crates/dnser-config/README.md) | TOML config loader with typed accessors and built-in defaults |

## Scope

`dnser` is a *forwarding* resolver — it answers by relaying queries to upstreams you configure (e.g. `1.1.1.1`, `8.8.8.8`), not by iterating the DNS hierarchy itself. There is no authoritative-server support; it does not serve zone files.

## License

MIT
</content>

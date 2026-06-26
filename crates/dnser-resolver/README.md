# dnser-resolver

Forwarding DNS resolver for [dnser](../../). Given a query, it races the request across every configured upstream and returns the first valid reply.

## What it does

- Opens a fresh UDP socket per attempt and assigns a **random** 16-bit query ID per RFC 5452 §9.2 to harden against off-path spoofing.
- Sends the question to every configured upstream **in parallel**; the first parseable reply whose ID and question match the request wins, and the rest are dropped.
- Falls through to the next attempt on timeout or transport error (`timeout_ms` per upstream).
- Falls back to TCP automatically when the UDP reply has the `TC` bit set (RFC 1035 §4.2.1).
- Returns `SERVFAIL` if every upstream times out or returns garbage.

## Non-goals

This is a **forwarding** resolver — it does not perform full recursive resolution. There is no root-hint following, no NS chasing, no CNAME unwinding beyond what the upstreams already do. Point it at recursive resolvers (e.g. `1.1.1.1`, `8.8.8.8`, or a local Unbound) and it will be happy.

DNSSEC validation is not performed in this crate; the `AD` bit is passed through from upstream responses unchanged.

## Usage

```rust
use dnser_resolver::Resolver;
use dnser_config::ResolverConfig;

let resolver = Resolver::new(ResolverConfig::default()).await?;
let response = resolver.resolve(&query).await?;
```

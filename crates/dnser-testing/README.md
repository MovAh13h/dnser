# dnser-testing

Test fixtures and mock upstreams shared across the [dnser](../../) integration tests. **Dev-dep only — never published**, never part of the production build graph. It exists so integration tests don't redefine the same scaffolding in every file.

## What's in it

- **Query builders** — `make_query(name, qtype)`, `make_edns_query(name, qtype, udp_size)`, `soa_record(zone, ttl, minimum)`.
- **Mock upstreams** — `spawn_udp_responder` / `spawn_udp_responder_counted`, `spawn_tcp_responder`, `spawn_dual_responder` (UDP + TCP on the same port).
- **One-shot clients** — `udp_query(server, msg)`, `tcp_query(server, msg)`.
- **Byte-level mocks** (`mocks::*`) — pre-built response bytes for common upstream behaviors: `echo`, `truncated`, `nxdomain`, `nodata`, `many_a_records(n)`.
- **Message-level fixtures** (`fixtures::*`) — typed `Message` builders for common scenarios: `question`, `a_record`, `noerror`, `servfail`, `truncated`, `nxdomain`, `nodata`.

UDP responders bind to `127.0.0.1:0`; the helpers return the assigned `SocketAddr` so tests can pass it to the system under test. Tasks live until the process exits — no cleanup needed.

# dnser-testing

Test fixtures and mock upstreams shared across the dnser integration tests. Dev-dep only — never published. Provides `spawn_udp_responder`, `spawn_udp_responder_counted`, and `make_query` so each test crate doesn't redefine the same scaffolding.

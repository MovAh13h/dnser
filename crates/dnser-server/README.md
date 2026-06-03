# dnser-server

Async UDP and TCP server runtime. Owns the socket listeners, handles connection lifecycle, reads incoming DNS queries, dispatches them to the resolver, and writes responses back. Responsible for enforcing DNS-over-UDP truncation (TC bit) and managing concurrent request limits.

# dnser-resolver

Recursive DNS resolver and forwarding logic. Given a query, it checks the cache first, then either answers from a local authoritative zone or forwards upstream, following CNAME chains and delegation as needed. Designed to support both full recursive resolution and simple stub/forwarding modes.

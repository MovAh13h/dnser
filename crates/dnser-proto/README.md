# dnser-proto

DNS wire-format implementation following RFC 1035 and friends. Handles parsing and serialization of DNS messages, headers, questions, and resource records (A, AAAA, CNAME, NS, MX, TXT, SOA, PTR, SRV). Has no async dependencies and no internal crate dependencies — it is the foundation everything else builds on.

# dnser-net

Async transport helpers shared by the dnser server and resolver. Today: DNS-over-TCP message framing per RFC 1035 §4.2.2 (2-byte big-endian length prefix). Generic over any `AsyncRead`/`AsyncWrite`, so the same code serves both the listener loop and one-shot client queries.
</content>

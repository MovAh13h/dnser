# dnser-net

Async transport helpers shared by the [dnser](../../) server and resolver.

Today this is just **DNS-over-TCP message framing** per RFC 1035 §4.2.2 (2-byte big-endian length prefix). The helpers are generic over any `AsyncRead`/`AsyncWrite`, so the same code serves the server's accept loop, the resolver's TCP fallback client, and the integration test scaffolding.

## Usage

```rust
use dnser_net::{read_framed, write_framed};

// Server side:
while let Some(body) = read_framed(&mut stream).await? {
    let response = handle(&body).await;
    write_framed(&mut stream, &response).await?;
}

// Client side:
write_framed(&mut stream, &query).await?;
let response = read_framed(&mut stream).await?.expect("connection closed");
```

`read_framed` returns `Ok(None)` on a clean EOF between messages.

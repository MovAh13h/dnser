# dnser-proto

DNS wire-format parsing and serialization for [dnser](../../). Implements RFC 1035 and the EDNS(0) extension (RFC 6891).

## What's covered

- **Messages**: `Message::parse(bytes)` and `Message::to_bytes()` round-trip the full DNS message — header, questions, answer/authority/additional sections.
- **Header**: `Header::reply_to(query, extra_flags)` constructs response headers correctly (echo ID, set QR=1, copy RD, set RA).
- **Record types**: `A`, `AAAA`, `CNAME`, `NS`, `MX`, `TXT`, `SOA`, `PTR`, `SRV`, plus a raw `Unknown` fallback for types this crate doesn't model.
- **Name compression**: parsing handles RFC 1035 §4.1.4 message-compression pointers (and rejects pointer loops).
- **EDNS(0)** (RFC 6891): `ResourceRecord::edns_opt(udp_size)` builds an OPT pseudo-RR; helpers on `ResourceRecord` expose `is_opt()`, `edns_udp_size()`, `edns_version()`, `edns_extended_rcode()` and setters for the last two.
- **Error types**: `ParseError` and `WriteError` are strongly typed and `Display` to actionable messages.

## Usage

```rust
use dnser_proto::Message;

let msg = Message::parse(&bytes)?;
println!("id={}, questions={}", msg.header.id, msg.questions.len());

let response = build_response(&msg);
let bytes = response.to_bytes()?;
```

## Constants

- `MAX_UDP_SIZE = 4096` — buffer size used by the server for receiving UDP datagrams. The on-the-wire UDP limit before truncation is `512` (RFC 1035 §4.2.1) unless EDNS(0) advertises larger.

//! One-shot upstream TCP queries — used as fallback when a UDP response
//! comes back with the TC (truncated) bit set (RFC 1035 §4.2.2).
//!
//! Per RFC 7766 a resolver SHOULD keep TCP connections open and pipeline,
//! but TC fallbacks are rare on a forwarding resolver pointed at modern
//! recursive upstreams (they advertise large EDNS(0) buffers), so the
//! simpler one-shot model is fine for v1. Pooling is a future refinement.

use std::net::SocketAddr;
use std::time::Duration;

use dnser_net::{read_framed, write_framed};
use dnser_proto::Message;
use tokio::net::TcpStream;

use crate::error::ResolveError;

/// Send a single DNS query over a fresh TCP connection and return the parsed
/// response. The whole exchange (connect + write + read) is bounded by
/// `timeout`.
pub(crate) async fn tcp_query(
    addr: SocketAddr,
    query: &Message,
    timeout: Duration,
) -> Result<Message, ResolveError> {
    match tokio::time::timeout(timeout, exchange(addr, query)).await {
        Ok(r) => r,
        Err(_) => Err(ResolveError::Timeout),
    }
}

async fn exchange(addr: SocketAddr, query: &Message) -> Result<Message, ResolveError> {
    let original_id = query.header.id;
    let bytes = query.to_bytes()?;

    let mut stream = TcpStream::connect(addr).await?;
    write_framed(&mut stream, &bytes).await?;

    let body = read_framed(&mut stream)
        .await?
        .ok_or(ResolveError::InvalidResponse)?;

    let mut msg = Message::parse(body)?;
    if !msg.header.is_response() || msg.questions != query.questions {
        return Err(ResolveError::InvalidResponse);
    }
    msg.header.id = original_id;
    Ok(msg)
}

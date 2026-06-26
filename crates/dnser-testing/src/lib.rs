//! Test fixtures and mock upstreams shared across dnser integration tests.
//!
//! This crate is dev-dep only — it is never published and is not part of the
//! production build graph. It exists solely so that the resolver and server
//! integration tests don't redefine the same `mock_upstream` and `make_query`
//! helpers in every file.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use dnser_proto::{Class, Header, Message, Question, RecordType};
use tokio::net::UdpSocket;

/// Spawns a mock UDP upstream that calls `respond` on every incoming datagram
/// and sends back whatever it returns. Returning an empty vec suppresses the
/// reply (useful for simulating timeouts).
///
/// The task lives until the returned socket is dropped, which happens when
/// the process exits. Tests don't need to clean up explicitly.
pub async fn spawn_udp_responder<F>(respond: F) -> SocketAddr
where
    F: Fn(&[u8]) -> Vec<u8> + Send + 'static,
{
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
            let resp = respond(&buf[..n]);
            if !resp.is_empty() {
                let _ = socket.send_to(&resp, peer).await;
            }
        }
    });
    addr
}

/// Like [`spawn_udp_responder`] but also returns a counter incremented on every
/// received query — useful for asserting cache hits (counter stays put on a
/// hit) and cache misses (counter ticks up).
pub async fn spawn_udp_responder_counted<F>(respond: F) -> (SocketAddr, Arc<AtomicUsize>)
where
    F: Fn(&[u8]) -> Vec<u8> + Send + 'static,
{
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let counter2 = Arc::clone(&counter);
    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
            counter2.fetch_add(1, Ordering::SeqCst);
            let resp = respond(&buf[..n]);
            if !resp.is_empty() {
                let _ = socket.send_to(&resp, peer).await;
            }
        }
    });
    (addr, counter)
}

/// Builds a single-question DNS query with `RD=1` and `id=1234`. Tests that
/// need a different id can mutate `msg.header.id` after the fact.
#[must_use]
pub fn make_query(name: &str, qtype: RecordType) -> Message {
    Message {
        header: Header {
            id: 1234,
            flags: Header::RD,
            qd_count: 1,
            ..Default::default()
        },
        questions: vec![Question {
            name: name.to_string(),
            qtype,
            qclass: Class::IN,
        }],
        ..Default::default()
    }
}

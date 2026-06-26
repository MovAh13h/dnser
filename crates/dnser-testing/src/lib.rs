//! Test fixtures and mock upstreams shared across dnser integration tests.
//!
//! This crate is dev-dep only — it is never published and is not part of the
//! production build graph. It exists so that integration tests across the
//! workspace don't redefine the same query builders, mock upstreams, and
//! one-shot clients in every file.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use dnser_net::{read_framed, write_framed};
use dnser_proto::{Class, Header, Message, Question, RData, RecordType, ResourceRecord};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

pub mod fixtures;
pub mod mocks;

// ── Query builders ───────────────────────────────────────────────────────────

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

/// Like [`make_query`] but also appends an EDNS(0) OPT pseudo-RR advertising
/// `udp_size`.
#[must_use]
pub fn make_edns_query(name: &str, qtype: RecordType, udp_size: u16) -> Message {
    let mut q = make_query(name, qtype);
    q.additional.push(ResourceRecord::edns_opt(udp_size));
    q.header.ar_count = 1;
    q
}

/// Builds a minimal SOA resource record for `zone` with the given TTL and
/// SOA `minimum`. Used by negative-cache fixtures.
#[must_use]
pub fn soa_record(zone: &str, ttl: u32, minimum: u32) -> ResourceRecord {
    ResourceRecord {
        name: zone.to_string(),
        class: Class::IN,
        ttl,
        rdata: RData::SOA {
            mname: format!("ns1.{zone}"),
            rname: format!("admin.{zone}"),
            serial: 1,
            refresh: 3600,
            retry: 600,
            expire: 86400,
            minimum,
        },
    }
}

// ── Mock upstreams ───────────────────────────────────────────────────────────

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

/// Spawns a mock TCP upstream using DNS-over-TCP length-prefix framing.
/// `respond` is invoked once per received message body; the returned bytes
/// are framed and written back.
pub async fn spawn_tcp_responder<F>(respond: F) -> SocketAddr
where
    F: Fn(&[u8]) -> Vec<u8> + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let respond = Arc::new(respond);
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => break,
            };
            let respond = Arc::clone(&respond);
            tokio::spawn(async move {
                while let Ok(Some(body)) = read_framed(&mut stream).await {
                    let resp = respond(&body);
                    if write_framed(&mut stream, &resp).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    addr
}

/// Binds a UDP socket and a TCP listener on the same port (separate transport
/// namespaces, so the OS allows this), and spawns a UDP responder plus a
/// TCP responder each driven by its own handler. Returns the shared address.
pub async fn spawn_dual_responder<U, T>(udp_respond: U, tcp_respond: T) -> SocketAddr
where
    U: Fn(&[u8]) -> Vec<u8> + Send + 'static,
    T: Fn(&[u8]) -> Vec<u8> + Send + Sync + 'static,
{
    // Bind TCP first to claim a port, then bind UDP on the same port.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let socket = UdpSocket::bind(addr).await.unwrap();

    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
            let resp = udp_respond(&buf[..n]);
            if !resp.is_empty() {
                let _ = socket.send_to(&resp, peer).await;
            }
        }
    });

    let tcp_respond = Arc::new(tcp_respond);
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => break,
            };
            let respond = Arc::clone(&tcp_respond);
            tokio::spawn(async move {
                while let Ok(Some(body)) = read_framed(&mut stream).await {
                    let resp = respond(&body);
                    if write_framed(&mut stream, &resp).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    addr
}

// ── One-shot clients ─────────────────────────────────────────────────────────

/// Sends `query` to `server` over UDP from an ephemeral socket and parses the
/// reply. Panics on I/O or parse errors.
pub async fn udp_query(server: SocketAddr, query: &Message) -> Message {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket
        .send_to(&query.to_bytes().unwrap(), server)
        .await
        .unwrap();
    let mut buf = [0u8; 4096];
    let (n, _) = socket.recv_from(&mut buf).await.unwrap();
    Message::try_from(&buf[..n]).unwrap()
}

/// Opens a fresh TCP connection to `server`, sends `query` using length-prefix
/// framing, and parses the reply. Panics on I/O or parse errors.
pub async fn tcp_query(server: SocketAddr, query: &Message) -> Message {
    let mut stream = TcpStream::connect(server).await.unwrap();
    let bytes = query.to_bytes().unwrap();
    write_framed(&mut stream, &bytes).await.unwrap();
    let body = read_framed(&mut stream)
        .await
        .unwrap()
        .expect("stream closed before a complete message arrived");
    Message::parse(body).unwrap()
}

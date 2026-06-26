use std::net::SocketAddr;

use dnser_config::ResolverConfig;
use dnser_proto::{Class, Header, Message, Question, RData, RecordType, ResourceRecord};
use dnser_resolver::{ResolveError, Resolver};
use dnser_testing::spawn_udp_responder as mock_upstream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};

fn make_query() -> Message {
    dnser_testing::make_query("example.com", RecordType::A)
}

// Returns a well-formed DNS response that mirrors the questions from the incoming query.
fn echo_as_response(query_bytes: &[u8]) -> Vec<u8> {
    let id = u16::from_be_bytes([query_bytes[0], query_bytes[1]]);
    let incoming = Message::try_from(query_bytes).unwrap();
    Message {
        header: Header {
            id,
            flags: Header::QR | Header::RD,
            qd_count: incoming.questions.len() as u16,
            ..Default::default()
        },
        questions: incoming.questions,
        ..Default::default()
    }
    .to_bytes()
    .unwrap()
    .to_vec()
}

// Returns a response that looks like a query (QR bit absent) — should be rejected.
fn echo_without_qr(query_bytes: &[u8]) -> Vec<u8> {
    let id = u16::from_be_bytes([query_bytes[0], query_bytes[1]]);
    let incoming = Message::try_from(query_bytes).unwrap();
    Message {
        header: Header {
            id,
            flags: Header::RD, // QR deliberately absent
            qd_count: incoming.questions.len() as u16,
            ..Default::default()
        },
        questions: incoming.questions,
        ..Default::default()
    }
    .to_bytes()
    .unwrap()
    .to_vec()
}

// Returns a valid response (QR set) but with the wrong question — should be rejected.
fn respond_with_wrong_question(query_bytes: &[u8]) -> Vec<u8> {
    let id = u16::from_be_bytes([query_bytes[0], query_bytes[1]]);
    Message {
        header: Header {
            id,
            flags: Header::QR | Header::RD,
            qd_count: 1,
            ..Default::default()
        },
        questions: vec![Question {
            name: "different.example.com".to_string(),
            qtype: RecordType::A,
            qclass: Class::IN,
        }],
        ..Default::default()
    }
    .to_bytes()
    .unwrap()
    .to_vec()
}

async fn resolver_with(addrs: Vec<SocketAddr>) -> Resolver {
    Resolver::new(ResolverConfig {
        upstreams: addrs,
        timeout_ms: 200,
    })
    .await
    .unwrap()
}

// --- tests ---

#[tokio::test]
async fn valid_response_accepted() {
    let addr = mock_upstream(echo_as_response).await;
    let resolver = resolver_with(vec![addr]).await;
    assert!(resolver.resolve(&make_query()).await.is_ok());
}

#[tokio::test]
async fn qr_bit_not_set_is_rejected() {
    let addr = mock_upstream(echo_without_qr).await;
    let resolver = resolver_with(vec![addr]).await;
    assert!(matches!(
        resolver.resolve(&make_query()).await,
        Err(ResolveError::AllFailed)
    ));
}

#[tokio::test]
async fn question_mismatch_is_rejected() {
    let addr = mock_upstream(respond_with_wrong_question).await;
    let resolver = resolver_with(vec![addr]).await;
    assert!(matches!(
        resolver.resolve(&make_query()).await,
        Err(ResolveError::AllFailed)
    ));
}

// Verifies the fan-out fallback: if the first upstream returns an invalid response the
// resolver should still succeed via the second upstream.
#[tokio::test]
async fn second_upstream_used_when_first_fails() {
    let bad = mock_upstream(echo_without_qr).await;
    let good = mock_upstream(echo_as_response).await;
    let resolver = resolver_with(vec![bad, good]).await;
    assert!(resolver.resolve(&make_query()).await.is_ok());
}

// The resolver rewrites the ID on the wire and must restore the original in the response.
// A DNS client validates that the response ID matches its query; a mismatch causes it to
// silently discard the answer.
#[tokio::test]
async fn response_id_matches_query() {
    let addr = mock_upstream(echo_as_response).await;
    let resolver = resolver_with(vec![addr]).await;
    let query = make_query();
    let response = resolver.resolve(&query).await.unwrap();
    assert_eq!(response.header.id, query.header.id);
}

#[tokio::test]
async fn no_upstreams_returns_error() {
    let resolver = resolver_with(vec![]).await;
    assert!(matches!(
        resolver.resolve(&make_query()).await,
        Err(ResolveError::NoUpstreams)
    ));
}

// --- TCP fallback tests ---

/// Bind a UDP socket and a TCP listener on the same port (separate transport
/// namespaces, so the OS lets this coexist). Returns the shared address.
async fn bind_pair() -> (UdpSocket, TcpListener, SocketAddr) {
    // Bind TCP first to claim a port, then bind UDP on the same port.
    let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = tcp.local_addr().unwrap();
    let udp = UdpSocket::bind(addr).await.unwrap();
    (udp, tcp, addr)
}

/// Build a response that echoes the question and sets TC=1 with no answers —
/// what an upstream sends when the answer didn't fit in UDP.
fn truncated_response(query_bytes: &[u8]) -> Vec<u8> {
    let id = u16::from_be_bytes([query_bytes[0], query_bytes[1]]);
    let incoming = Message::try_from(query_bytes).unwrap();
    Message {
        header: Header {
            id,
            flags: Header::QR | Header::TC | Header::RD,
            qd_count: incoming.questions.len() as u16,
            ..Default::default()
        },
        questions: incoming.questions,
        ..Default::default()
    }
    .to_bytes()
    .unwrap()
    .to_vec()
}

/// Build a "full" response with one A record — what an upstream returns once
/// queried over TCP.
fn full_response(query_bytes: &[u8]) -> Vec<u8> {
    let id = u16::from_be_bytes([query_bytes[0], query_bytes[1]]);
    let incoming = Message::try_from(query_bytes).unwrap();
    let q = incoming.questions[0].clone();
    Message {
        header: Header {
            id,
            flags: Header::QR | Header::RD,
            qd_count: 1,
            an_count: 1,
            ..Default::default()
        },
        questions: vec![q.clone()],
        answers: vec![ResourceRecord {
            name: q.name,
            class: Class::IN,
            ttl: 60,
            rdata: RData::A(std::net::Ipv4Addr::new(1, 2, 3, 4)),
        }],
        ..Default::default()
    }
    .to_bytes()
    .unwrap()
    .to_vec()
}

#[tokio::test]
async fn tc_response_triggers_tcp_fallback() {
    let (udp, tcp, addr) = bind_pair().await;

    // UDP side: always reply with TC=1.
    tokio::spawn(async move {
        let mut buf = [0u8; 512];
        while let Ok((n, peer)) = udp.recv_from(&mut buf).await {
            let resp = truncated_response(&buf[..n]);
            let _ = udp.send_to(&resp, peer).await;
        }
    });
    // TCP side: read length-prefixed query, write length-prefixed full response.
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match tcp.accept().await {
                Ok(p) => p,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut len_buf = [0u8; 2];
                if stream.read_exact(&mut len_buf).await.is_err() {
                    return;
                }
                let n = u16::from_be_bytes(len_buf) as usize;
                let mut body = vec![0u8; n];
                if stream.read_exact(&mut body).await.is_err() {
                    return;
                }
                let resp = full_response(&body);
                let len = (resp.len() as u16).to_be_bytes();
                let _ = stream.write_all(&len).await;
                let _ = stream.write_all(&resp).await;
            });
        }
    });

    let resolver = resolver_with(vec![addr]).await;
    let response = resolver.resolve(&make_query()).await.unwrap();
    assert!(!response.header.is_truncated());
    assert_eq!(response.answers.len(), 1);
    match &response.answers[0].rdata {
        RData::A(ip) => assert_eq!(*ip, std::net::Ipv4Addr::new(1, 2, 3, 4)),
        _ => panic!("expected A record"),
    }
    // ID must still be restored to the caller's original.
    assert_eq!(response.header.id, make_query().header.id);
}

#[tokio::test]
async fn tc_with_failing_tcp_propagates_as_upstream_failure() {
    // UDP returns TC=1 but nothing is listening on TCP — the fallback connect
    // fails and the resolver bubbles up `AllFailed` because there is no other
    // upstream to try.
    let (udp, tcp, addr) = bind_pair().await;
    drop(tcp); // immediately stop accepting TCP connections
    tokio::spawn(async move {
        let mut buf = [0u8; 512];
        while let Ok((n, peer)) = udp.recv_from(&mut buf).await {
            let resp = truncated_response(&buf[..n]);
            let _ = udp.send_to(&resp, peer).await;
        }
    });

    let resolver = resolver_with(vec![addr]).await;
    assert!(matches!(
        resolver.resolve(&make_query()).await,
        Err(ResolveError::AllFailed)
    ));
}

#[tokio::test]
async fn good_upstream_used_when_other_tc_then_tcp_fails() {
    // First upstream: UDP returns TC=1, TCP is unreachable.
    // Second upstream: returns a clean answer over UDP.
    // Resolver should succeed via the second upstream.
    let (udp_bad, tcp_bad, bad_addr) = bind_pair().await;
    drop(tcp_bad);
    tokio::spawn(async move {
        let mut buf = [0u8; 512];
        while let Ok((n, peer)) = udp_bad.recv_from(&mut buf).await {
            let resp = truncated_response(&buf[..n]);
            let _ = udp_bad.send_to(&resp, peer).await;
        }
    });
    let good_addr = mock_upstream(echo_as_response).await;

    let resolver = resolver_with(vec![bad_addr, good_addr]).await;
    assert!(resolver.resolve(&make_query()).await.is_ok());
}

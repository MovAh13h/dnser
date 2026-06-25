use std::net::SocketAddr;

use dnser_config::ResolverConfig;
use dnser_proto::{Class, Header, Message, Question, RecordType};
use dnser_resolver::{ResolveError, Resolver};

// Spawns a mock UDP server. For each incoming packet, `respond` is called with the
// raw bytes; returning a non-empty vec causes that vec to be sent back as the response.
async fn mock_upstream(respond: impl Fn(&[u8]) -> Vec<u8> + Send + 'static) -> SocketAddr {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    tokio::spawn(async move {
        let mut buf = [0u8; 512];
        while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
            let response = respond(&buf[..n]);
            if !response.is_empty() {
                let _ = socket.send_to(&response, peer).await;
            }
        }
    });
    addr
}

fn make_query() -> Message {
    Message {
        header: Header {
            id: 1234,
            qd_count: 1,
            ..Default::default()
        },
        questions: vec![Question {
            name: "example.com".to_string(),
            qtype: RecordType::A,
            qclass: Class::IN,
        }],
        ..Default::default()
    }
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

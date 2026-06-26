use std::net::SocketAddr;

use dnser_config::ResolverConfig;
use dnser_proto::{Class, Header, Message, Question, RData, RecordType};
use dnser_resolver::{ResolveError, Resolver};
use dnser_testing::{
    make_query, mocks, spawn_dual_responder, spawn_udp_responder as mock_upstream,
};

fn query() -> Message {
    make_query("example.com", RecordType::A)
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
    let addr = mock_upstream(mocks::echo).await;
    let resolver = resolver_with(vec![addr]).await;
    assert!(resolver.resolve(&query()).await.is_ok());
}

#[tokio::test]
async fn qr_bit_not_set_is_rejected() {
    let addr = mock_upstream(echo_without_qr).await;
    let resolver = resolver_with(vec![addr]).await;
    assert!(matches!(
        resolver.resolve(&query()).await,
        Err(ResolveError::AllFailed)
    ));
}

#[tokio::test]
async fn question_mismatch_is_rejected() {
    let addr = mock_upstream(respond_with_wrong_question).await;
    let resolver = resolver_with(vec![addr]).await;
    assert!(matches!(
        resolver.resolve(&query()).await,
        Err(ResolveError::AllFailed)
    ));
}

// Verifies the fan-out fallback: if the first upstream returns an invalid response the
// resolver should still succeed via the second upstream.
#[tokio::test]
async fn second_upstream_used_when_first_fails() {
    let bad = mock_upstream(echo_without_qr).await;
    let good = mock_upstream(mocks::echo).await;
    let resolver = resolver_with(vec![bad, good]).await;
    assert!(resolver.resolve(&query()).await.is_ok());
}

// The resolver rewrites the ID on the wire and must restore the original in the response.
// A DNS client validates that the response ID matches its query; a mismatch causes it to
// silently discard the answer.
#[tokio::test]
async fn response_id_matches_query() {
    let addr = mock_upstream(mocks::echo).await;
    let resolver = resolver_with(vec![addr]).await;
    let q = query();
    let response = resolver.resolve(&q).await.unwrap();
    assert_eq!(response.header.id, q.header.id);
}

#[tokio::test]
async fn no_upstreams_returns_error() {
    let resolver = resolver_with(vec![]).await;
    assert!(matches!(
        resolver.resolve(&query()).await,
        Err(ResolveError::NoUpstreams)
    ));
}

// --- TCP fallback tests ---

#[tokio::test]
async fn tc_response_triggers_tcp_fallback() {
    // UDP advertises truncation, TCP returns a full A record.
    let addr = spawn_dual_responder(mocks::truncated, mocks::many_a_records(1)).await;

    let resolver = resolver_with(vec![addr]).await;
    let response = resolver.resolve(&query()).await.unwrap();
    assert!(!response.header.is_truncated());
    assert_eq!(response.answers.len(), 1);
    assert!(matches!(response.answers[0].rdata, RData::A(_)));
    // ID must still be restored to the caller's original.
    assert_eq!(response.header.id, query().header.id);
}

#[tokio::test]
async fn tc_with_failing_tcp_propagates_as_upstream_failure() {
    // UDP returns TC=1 but nothing is listening on TCP — the fallback connect
    // fails and the resolver bubbles up `AllFailed` because there is no other
    // upstream to try.
    let addr = mock_upstream(mocks::truncated).await;
    let resolver = resolver_with(vec![addr]).await;
    assert!(matches!(
        resolver.resolve(&query()).await,
        Err(ResolveError::AllFailed)
    ));
}

#[tokio::test]
async fn good_upstream_used_when_other_tc_then_tcp_fails() {
    // First upstream: UDP returns TC=1, TCP is unreachable.
    // Second upstream: returns a clean answer over UDP.
    // Resolver should succeed via the second upstream.
    let bad_addr = mock_upstream(mocks::truncated).await;
    let good_addr = mock_upstream(mocks::echo).await;

    let resolver = resolver_with(vec![bad_addr, good_addr]).await;
    assert!(resolver.resolve(&query()).await.is_ok());
}

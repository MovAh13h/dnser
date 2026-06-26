//! Stateless response builders for mock upstreams.
//!
//! Each function takes the raw bytes of an incoming query and returns the
//! bytes of a well-formed DNS response. They are all `fn` pointers so they
//! can be passed directly to [`crate::spawn_udp_responder`] and friends:
//!
//! ```ignore
//! let addr = spawn_udp_responder(mocks::echo).await;
//! ```

use std::net::Ipv4Addr;

use dnser_proto::{Class, Header, Message, RData, Rcode, ResourceRecord};

use crate::soa_record;

/// Minimal echo: `QR | RA | (query RD)`, copies questions, no answers.
#[must_use]
pub fn echo(query_bytes: &[u8]) -> Vec<u8> {
    let query = Message::try_from(query_bytes).unwrap();
    Message {
        header: Header::reply_to(&query.header, Header::RA),
        questions: query.questions,
        ..Default::default()
    }
    .to_bytes()
    .unwrap()
    .to_vec()
}

/// TC=1, no answers — what an upstream sends when the UDP answer didn't fit.
#[must_use]
pub fn truncated(query_bytes: &[u8]) -> Vec<u8> {
    let query = Message::try_from(query_bytes).unwrap();
    Message {
        header: Header::reply_to(&query.header, Header::TC),
        questions: query.questions,
        ..Default::default()
    }
    .to_bytes()
    .unwrap()
    .to_vec()
}

/// NXDOMAIN with a synthetic SOA (TTL=60, minimum=60) in the authority section.
#[must_use]
pub fn nxdomain(query_bytes: &[u8]) -> Vec<u8> {
    let query = Message::try_from(query_bytes).unwrap();
    let zone = query
        .questions
        .first()
        .map(|q| q.name.clone())
        .unwrap_or_else(|| "example.com".to_string());
    let mut header = Header::reply_to(&query.header, Header::RA | (Rcode::NXDomain as u16));
    header.ns_count = 1;
    Message {
        header,
        questions: query.questions,
        authority: vec![soa_record(&zone, 60, 60)],
        ..Default::default()
    }
    .to_bytes()
    .unwrap()
    .to_vec()
}

/// NODATA (NOERROR + empty answers) with a synthetic SOA (TTL=60, minimum=60).
#[must_use]
pub fn nodata(query_bytes: &[u8]) -> Vec<u8> {
    let query = Message::try_from(query_bytes).unwrap();
    let zone = query
        .questions
        .first()
        .map(|q| q.name.clone())
        .unwrap_or_else(|| "example.com".to_string());
    let mut header = Header::reply_to(&query.header, Header::RA);
    header.ns_count = 1;
    Message {
        header,
        questions: query.questions,
        authority: vec![soa_record(&zone, 60, 60)],
        ..Default::default()
    }
    .to_bytes()
    .unwrap()
    .to_vec()
}

/// Returns a responder that emits `n` A records (10.0.0.0..10.0.0.n) with
/// TTL=300 for any query. Useful for forcing UDP truncation.
pub fn many_a_records(n: u8) -> impl Fn(&[u8]) -> Vec<u8> + Send + Sync + 'static {
    move |query_bytes: &[u8]| {
        let query = Message::try_from(query_bytes).unwrap();
        let name = query
            .questions
            .first()
            .map(|q| q.name.clone())
            .unwrap_or_default();
        let answers: Vec<ResourceRecord> = (0..n)
            .map(|i| ResourceRecord {
                name: name.clone(),
                class: Class::IN,
                ttl: 300,
                rdata: RData::A(Ipv4Addr::new(10, 0, 0, i)),
            })
            .collect();
        let mut header = Header::reply_to(&query.header, Header::RA);
        header.an_count = answers.len() as u16;
        Message {
            header,
            questions: query.questions,
            answers,
            ..Default::default()
        }
        .to_bytes()
        .unwrap()
        .to_vec()
    }
}

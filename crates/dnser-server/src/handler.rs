use dnser_cache::Cache;
use dnser_proto::{Header, Message, Rcode};
use dnser_resolver::Resolver;
use tracing::{debug, warn};

/// Traditional DNS UDP payload limit before EDNS negotiation (RFC 1035).
pub(crate) const MAX_UDP_PAYLOAD: usize = 512;

/// Resolves `query` against the cache and upstream, always returning a response.
/// Upstream failures produce SERVFAIL rather than propagating an error.
pub(crate) async fn process_query(resolver: &Resolver, cache: &Cache, query: &Message) -> Message {
    if let [question] = query.questions.as_slice() {
        if let Some(mut cached) = cache.get(question) {
            cached.header.id = query.header.id;
            cached.header.flags =
                (cached.header.flags & !Header::RD) | (query.header.flags & Header::RD);
            debug!(id = query.header.id, "cache hit");
            return cached;
        }
    }

    match resolver.resolve(query).await {
        Ok(msg) => {
            cache.insert(&msg);
            msg
        }
        Err(e) => {
            warn!(id = query.header.id, err = %e, "resolution failed");
            build_servfail(query)
        }
    }
}

pub(crate) fn build_servfail(query: &Message) -> Message {
    Message {
        header: Header {
            id: query.header.id,
            flags: Header::QR | (Rcode::ServFail as u16) | (query.header.flags & Header::RD),
            qd_count: query.header.qd_count,
            ..Default::default()
        },
        questions: query.questions.clone(),
        ..Default::default()
    }
}

/// Builds a TC=1 response with no answers, telling the client to retry over TCP.
pub(crate) fn build_truncated(query: &Message) -> Message {
    Message {
        header: Header {
            id: query.header.id,
            flags: Header::QR | Header::TC | (query.header.flags & Header::RD),
            qd_count: query.header.qd_count,
            ..Default::default()
        },
        questions: query.questions.clone(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dnser_proto::{Class, Question, RecordType};

    fn query(id: u16, rd: bool) -> Message {
        Message {
            header: Header {
                id,
                flags: if rd { Header::RD } else { 0 },
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

    #[test]
    fn servfail_is_a_response_with_servfail_rcode() {
        let resp = build_servfail(&query(0, false));
        assert!(resp.header.is_response());
        assert_eq!(resp.header.rcode(), Ok(Rcode::ServFail));
    }

    #[test]
    fn servfail_echoes_id_and_questions() {
        let q = query(42, false);
        let resp = build_servfail(&q);
        assert_eq!(resp.header.id, 42);
        assert_eq!(resp.questions, q.questions);
    }

    #[test]
    fn servfail_copies_rd_bit() {
        assert!(build_servfail(&query(0, true)).header.recursion_desired());
        assert!(!build_servfail(&query(0, false)).header.recursion_desired());
    }

    #[test]
    fn truncated_sets_tc_and_qr_bits() {
        let resp = build_truncated(&query(7, true));
        assert!(resp.header.is_response());
        assert!(resp.header.is_truncated());
        assert_eq!(resp.header.id, 7);
        assert!(resp.header.recursion_desired());
        assert!(resp.answers.is_empty());
        assert!(resp.authority.is_empty());
        assert!(resp.additional.is_empty());
        assert_eq!(resp.questions, query(7, true).questions);
    }

    #[test]
    fn truncated_does_not_set_servfail() {
        let resp = build_truncated(&query(0, false));
        assert_eq!(resp.header.rcode(), Ok(Rcode::NoError));
    }
}

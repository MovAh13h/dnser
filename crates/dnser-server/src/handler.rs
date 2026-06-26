use dnser_cache::Cache;
use dnser_proto::{Header, Message, Rcode, ResourceRecord};
use dnser_resolver::Resolver;
use tracing::{debug, warn};

/// Traditional DNS UDP payload limit before EDNS negotiation (RFC 1035).
pub(crate) const MAX_UDP_PAYLOAD: usize = 512;

/// Our advertised EDNS UDP payload size.
const SERVER_UDP_PAYLOAD: u16 = dnser_proto::MAX_UDP_SIZE as u16;

/// Returns the effective UDP payload limit for a query: the client's advertised
/// size clamped to [512, MAX_UDP_SIZE], or 512 if no OPT is present.
pub(crate) fn query_udp_limit(query: &Message) -> usize {
    query
        .opt()
        .and_then(ResourceRecord::edns_udp_size)
        .map(|advertised| (advertised as usize).clamp(MAX_UDP_PAYLOAD, dnser_proto::MAX_UDP_SIZE))
        .unwrap_or(MAX_UDP_PAYLOAD)
}

/// Strips any upstream OPT and appends our own advertising SERVER_UDP_PAYLOAD.
fn add_edns_opt(response: &mut Message) {
    response.additional.retain(|rr| !rr.is_opt());
    response
        .additional
        .push(ResourceRecord::edns_opt(SERVER_UDP_PAYLOAD));
    response.header.ar_count = response.additional.len() as u16;
}

/// Resolves `query` against the cache and upstream, always returning a response.
/// Upstream failures produce SERVFAIL rather than propagating an error.
/// When the query carries an OPT record, the response includes one too.
pub(crate) async fn process_query(resolver: &Resolver, cache: &Cache, query: &Message) -> Message {
    // EDNS version check — only version 0 is defined (RFC 6891 §6.1.3).
    if let Some(opt) = query.opt() {
        if opt.edns_version() != Some(0) {
            return build_badvers(query);
        }
    }

    let has_opt = query.opt().is_some();

    if let [question] = query.questions.as_slice() {
        if let Some(mut cached) = cache.get(question) {
            cached.header.id = query.header.id;
            cached.header.flags =
                (cached.header.flags & !Header::RD) | (query.header.flags & Header::RD);
            if has_opt {
                add_edns_opt(&mut cached);
            }
            debug!(id = query.header.id, "cache hit");
            return cached;
        }
    }

    match resolver.resolve(query).await {
        Ok(mut msg) => {
            cache.insert(&msg);
            if has_opt {
                add_edns_opt(&mut msg);
            }
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
        header: Header::reply_to(&query.header, Rcode::ServFail as u16),
        questions: query.questions.clone(),
        ..Default::default()
    }
}

/// Builds a TC=1 response with no answers, telling the client to retry over TCP.
pub(crate) fn build_truncated(query: &Message) -> Message {
    Message {
        header: Header::reply_to(&query.header, Header::TC),
        questions: query.questions.clone(),
        ..Default::default()
    }
}

/// Builds a BADVERS response (RFC 6891 §6.1.3).
/// Header RCODE=0; extended RCODE=1 is encoded in OPT TTL byte 0.
fn build_badvers(query: &Message) -> Message {
    let mut header = Header::reply_to(&query.header, 0);
    header.ar_count = 1;
    let mut opt = ResourceRecord::edns_opt(SERVER_UDP_PAYLOAD);
    opt.set_edns_extended_rcode(1);
    Message {
        header,
        questions: query.questions.clone(),
        additional: vec![opt],
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

    fn query_with_opt(id: u16, udp_size: u16) -> Message {
        let mut q = query(id, true);
        q.additional.push(ResourceRecord::edns_opt(udp_size));
        q.header.ar_count = 1;
        q
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

    #[test]
    fn query_udp_limit_no_opt_returns_512() {
        assert_eq!(query_udp_limit(&query(1, true)), 512);
    }

    #[test]
    fn query_udp_limit_opt_clamped_below_512() {
        assert_eq!(query_udp_limit(&query_with_opt(1, 256)), 512);
    }

    #[test]
    fn query_udp_limit_opt_above_max_clamped() {
        assert_eq!(
            query_udp_limit(&query_with_opt(1, 65535)),
            dnser_proto::MAX_UDP_SIZE
        );
    }

    #[test]
    fn query_udp_limit_opt_1280_roundtrips() {
        assert_eq!(query_udp_limit(&query_with_opt(1, 1280)), 1280);
    }

    #[test]
    fn badvers_has_opt_with_extended_rcode_1() {
        let resp = build_badvers(&query(9, true));
        assert!(resp.header.is_response());
        let opt = resp.opt().expect("OPT record must be present in BADVERS");
        assert_eq!(opt.edns_extended_rcode(), Some(1));
    }
}

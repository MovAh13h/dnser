use dnser_proto::{Header, Message, Rcode};

// QR=1, RCODE=SERVFAIL(2) — returned when all upstreams fail or timeout.
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
}

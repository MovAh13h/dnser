use std::net::Ipv4Addr;

use dnser_proto::{Class, Header, Message, RData, ResourceRecord};

const DEMO_IP: Ipv4Addr = Ipv4Addr::new(1, 2, 3, 4);
const DEMO_TTL: u32 = 60;

pub(crate) fn build_response(query: Message) -> Message {
    let flags = 0b1000_0100_0000_0000u16 | (query.header.flags & 0b0000_0001_0000_0000); // QR=1, AA=1, RD copied

    let answers: Vec<ResourceRecord> = query
        .questions
        .iter()
        .map(|q| ResourceRecord {
            name: q.name.clone(),
            class: Class::IN,
            ttl: DEMO_TTL,
            rdata: RData::A(DEMO_IP),
        })
        .collect();

    Message {
        header: Header {
            id: query.header.id,
            flags,
            qd_count: query.header.qd_count,
            an_count: answers.len() as u16,
            ns_count: 0,
            ar_count: 0,
        },
        questions: query.questions,
        answers,
        authority: vec![],
        additional: vec![],
    }
}

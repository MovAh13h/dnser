use bytes::Bytes;
use dnser_proto::{Class, Header, Message, Question, RData, RecordType, ResourceRecord};
use proptest::prelude::*;
use std::net::{Ipv4Addr, Ipv6Addr};

fn dns_name() -> impl Strategy<Value = String> {
    prop::collection::vec("[a-z]{1,8}", 1..=3usize).prop_map(|labels| labels.join("."))
}

fn arb_class() -> impl Strategy<Value = Class> {
    prop_oneof![
        Just(Class::IN),
        Just(Class::CS),
        Just(Class::CH),
        Just(Class::HS),
    ]
}

fn arb_record_type() -> impl Strategy<Value = RecordType> {
    prop_oneof![
        Just(RecordType::A),
        Just(RecordType::AAAA),
        Just(RecordType::NS),
        Just(RecordType::CNAME),
        Just(RecordType::SOA),
        Just(RecordType::PTR),
        Just(RecordType::MX),
        Just(RecordType::TXT),
        Just(RecordType::SRV),
        Just(RecordType::IXFR),
        Just(RecordType::AXFR),
        Just(RecordType::ANY),
    ]
}

fn arb_rdata() -> impl Strategy<Value = RData> {
    prop_oneof![
        any::<[u8; 4]>().prop_map(|b| RData::A(Ipv4Addr::from(b))),
        any::<[u8; 16]>().prop_map(|b| RData::AAAA(Ipv6Addr::from(b))),
        dns_name().prop_map(RData::NS),
        dns_name().prop_map(RData::CNAME),
        dns_name().prop_map(RData::PTR),
        (any::<u16>(), dns_name()).prop_map(|(preference, exchange)| RData::MX {
            preference,
            exchange
        }),
        prop::collection::vec(
            prop::collection::vec(any::<u8>(), 0..=255usize).prop_map(Bytes::from),
            0..=4usize,
        )
        .prop_map(RData::TXT),
        (any::<u16>(), any::<u16>(), any::<u16>(), dns_name()).prop_map(
            |(priority, weight, port, target)| RData::SRV {
                priority,
                weight,
                port,
                target,
            },
        ),
        (
            dns_name(),
            dns_name(),
            any::<u32>(),
            any::<u32>(),
            any::<u32>(),
            any::<u32>(),
            any::<u32>(),
        )
            .prop_map(|(mname, rname, serial, refresh, retry, expire, minimum)| {
                RData::SOA {
                    mname,
                    rname,
                    serial,
                    refresh,
                    retry,
                    expire,
                    minimum,
                }
            }),
        // rtype 100-200: not a known RecordType, always parses back as Unknown
        (
            100u16..=200u16,
            prop::collection::vec(any::<u8>(), 0..=32usize)
        )
            .prop_map(|(rtype, data)| RData::Unknown {
                rtype,
                data: Bytes::from(data),
            }),
    ]
}

fn arb_question() -> impl Strategy<Value = Question> {
    (dns_name(), arb_record_type(), arb_class()).prop_map(|(name, qtype, qclass)| Question {
        name,
        qtype,
        qclass,
    })
}

fn arb_resource_record() -> impl Strategy<Value = ResourceRecord> {
    (dns_name(), arb_class(), any::<u32>(), arb_rdata()).prop_map(|(name, class, ttl, rdata)| {
        ResourceRecord {
            name,
            class,
            ttl,
            rdata,
        }
    })
}

fn arb_message() -> impl Strategy<Value = Message> {
    (
        any::<u16>(),
        any::<u16>().prop_map(|f| f & !0x0040u16), // Z bit must be clear
        prop::collection::vec(arb_question(), 0..=3usize),
        prop::collection::vec(arb_resource_record(), 0..=4usize),
        prop::collection::vec(arb_resource_record(), 0..=2usize),
        prop::collection::vec(arb_resource_record(), 0..=2usize),
    )
        .prop_map(
            |(id, flags, questions, answers, authority, additional)| Message {
                header: Header {
                    id,
                    flags,
                    qd_count: questions.len() as u16,
                    an_count: answers.len() as u16,
                    ns_count: authority.len() as u16,
                    ar_count: additional.len() as u16,
                },
                questions,
                answers,
                authority,
                additional,
            },
        )
}

proptest! {
    #[test]
    fn roundtrip_message(msg in arb_message()) {
        let bytes = msg.to_bytes().unwrap();
        let parsed = Message::parse(bytes).unwrap();
        prop_assert_eq!(msg, parsed);
    }
}

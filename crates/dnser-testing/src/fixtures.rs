//! Pre-built typed `Message` fixtures for unit tests that exercise DNS logic
//! directly (rather than going through the wire).
//!
//! Use [`crate::mocks`] when you need bytes-in / bytes-out responders to pass
//! to [`crate::spawn_udp_responder`] and friends. Use this module when you
//! need a typed [`Message`] to feed straight into something like
//! `Cache::insert`.

use std::net::Ipv4Addr;

use dnser_proto::{Class, Header, Message, Question, RData, Rcode, RecordType, ResourceRecord};

/// A Question with class IN.
#[must_use]
pub fn question(name: &str, qtype: RecordType) -> Question {
    Question {
        name: name.to_string(),
        qtype,
        qclass: Class::IN,
    }
}

/// An A record at `ip` with the given TTL and class IN.
#[must_use]
pub fn a_record(name: &str, ip: Ipv4Addr, ttl: u32) -> ResourceRecord {
    ResourceRecord {
        name: name.to_string(),
        class: Class::IN,
        ttl,
        rdata: RData::A(ip),
    }
}

/// A NOERROR response (QR | RD | RA) carrying the given question and answers.
/// Counts are populated from the vec lengths.
#[must_use]
pub fn noerror(q: Question, answers: Vec<ResourceRecord>) -> Message {
    let an_count = answers.len() as u16;
    Message {
        header: Header {
            id: 1,
            flags: Header::QR | Header::RD | Header::RA,
            qd_count: 1,
            an_count,
            ..Default::default()
        },
        questions: vec![q],
        answers,
        ..Default::default()
    }
}

/// A SERVFAIL response with no RD/RA and no answers.
#[must_use]
pub fn servfail(q: Question) -> Message {
    Message {
        header: Header {
            id: 1,
            flags: Header::QR | (Rcode::ServFail as u16),
            qd_count: 1,
            ..Default::default()
        },
        questions: vec![q],
        ..Default::default()
    }
}

/// A truncated NOERROR response (TC=1) with the given question and answers.
#[must_use]
pub fn truncated(q: Question, answers: Vec<ResourceRecord>) -> Message {
    let an_count = answers.len() as u16;
    Message {
        header: Header {
            id: 1,
            flags: Header::QR | Header::TC | Header::RD,
            qd_count: 1,
            an_count,
            ..Default::default()
        },
        questions: vec![q],
        answers,
        ..Default::default()
    }
}

/// An NXDOMAIN response with `soa` in the authority section.
#[must_use]
pub fn nxdomain(q: Question, soa: ResourceRecord) -> Message {
    Message {
        header: Header {
            id: 1,
            flags: Header::QR | (Rcode::NXDomain as u16),
            qd_count: 1,
            ns_count: 1,
            ..Default::default()
        },
        questions: vec![q],
        authority: vec![soa],
        ..Default::default()
    }
}

/// A NODATA response (NOERROR + empty answers) with `soa` in authority.
#[must_use]
pub fn nodata(q: Question, soa: ResourceRecord) -> Message {
    Message {
        header: Header {
            id: 1,
            flags: Header::QR | Header::RA,
            qd_count: 1,
            ns_count: 1,
            ..Default::default()
        },
        questions: vec![q],
        authority: vec![soa],
        ..Default::default()
    }
}

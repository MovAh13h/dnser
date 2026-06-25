use std::fs;
use std::path::Path;

use bytes::Bytes;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    description: String,
    hex: String,
    expect: Option<Expect>,
}

#[derive(Deserialize, Default)]
struct Expect {
    id: Option<u16>,
    qr: Option<bool>,
    aa: Option<bool>,
    rd: Option<bool>,
    ra: Option<bool>,
    qd_count: Option<u16>,
    an_count: Option<u16>,
    ns_count: Option<u16>,
    ar_count: Option<u16>,
    questions: Option<Vec<ExpectQuestion>>,
    answers: Option<Vec<ExpectRecord>>,
}

#[derive(Deserialize)]
struct ExpectQuestion {
    name: Option<String>,
    qtype: Option<String>,
}

#[derive(Deserialize)]
struct ExpectRecord {
    name: Option<String>,
    ttl: Option<u32>,
    rtype: Option<String>,
    ipv4: Option<String>,
    ipv6: Option<String>,
    cname: Option<String>,
    ns: Option<String>,
    ptr: Option<String>,
    mx_pref: Option<u16>,
    mx_exchange: Option<String>,
    txt_chunks: Option<usize>,
    srv_priority: Option<u16>,
    srv_weight: Option<u16>,
    srv_port: Option<u16>,
    srv_target: Option<String>,
    soa_mname: Option<String>,
    soa_rname: Option<String>,
    soa_serial: Option<u32>,
}

fn decode_hex(s: &str) -> Vec<u8> {
    let clean: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    assert!(clean.len() % 2 == 0, "odd-length hex string");
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).unwrap())
        .collect()
}

fn run_fixture(path: &Path) {
    let source = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let fixture: Fixture =
        toml::from_str(&source).unwrap_or_else(|e| panic!("parse TOML {path:?}: {e}"));

    let raw = Bytes::from(decode_hex(&fixture.hex));

    let msg = dnser_proto::Message::parse(raw)
        .unwrap_or_else(|e| panic!("{}: parse failed: {e}", fixture.description));

    let serialized = msg
        .to_bytes()
        .unwrap_or_else(|e| panic!("{}: to_bytes failed: {e}", fixture.description));
    let msg2 = dnser_proto::Message::parse(serialized)
        .unwrap_or_else(|e| panic!("{}: round-trip parse failed: {e}", fixture.description));
    assert_eq!(msg, msg2, "{}: round-trip mismatch", fixture.description);

    if let Some(exp) = &fixture.expect {
        assert_expect(exp, &msg, &fixture.description);
    }
}

fn assert_expect(exp: &Expect, msg: &dnser_proto::Message, desc: &str) {
    if let Some(id) = exp.id {
        assert_eq!(msg.header.id, id, "{desc}: id");
    }
    if let Some(qr) = exp.qr {
        assert_eq!(msg.header.is_response(), qr, "{desc}: qr");
    }
    if let Some(aa) = exp.aa {
        assert_eq!(msg.header.is_authoritative(), aa, "{desc}: aa");
    }
    if let Some(rd) = exp.rd {
        assert_eq!(msg.header.recursion_desired(), rd, "{desc}: rd");
    }
    if let Some(ra) = exp.ra {
        assert_eq!(msg.header.recursion_available(), ra, "{desc}: ra");
    }
    if let Some(qd) = exp.qd_count {
        assert_eq!(msg.header.qd_count, qd, "{desc}: qd_count");
    }
    if let Some(an) = exp.an_count {
        assert_eq!(msg.header.an_count, an, "{desc}: an_count");
    }
    if let Some(ns) = exp.ns_count {
        assert_eq!(msg.header.ns_count, ns, "{desc}: ns_count");
    }
    if let Some(ar) = exp.ar_count {
        assert_eq!(msg.header.ar_count, ar, "{desc}: ar_count");
    }

    if let Some(questions) = &exp.questions {
        assert_eq!(
            msg.questions.len(),
            questions.len(),
            "{desc}: question count"
        );
        for (i, eq) in questions.iter().enumerate() {
            let q = &msg.questions[i];
            if let Some(name) = &eq.name {
                assert_eq!(&q.name, name, "{desc}: question[{i}].name");
            }
            if let Some(qt) = &eq.qtype {
                assert_eq!(&format!("{:?}", q.qtype), qt, "{desc}: question[{i}].qtype");
            }
        }
    }

    if let Some(answers) = &exp.answers {
        assert_eq!(msg.answers.len(), answers.len(), "{desc}: answer count");
        for (i, er) in answers.iter().enumerate() {
            assert_record(er, &msg.answers[i], desc, &format!("answer[{i}]"));
        }
    }
}

fn assert_record(exp: &ExpectRecord, rr: &dnser_proto::ResourceRecord, desc: &str, label: &str) {
    use dnser_proto::RData;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use std::str::FromStr;

    if let Some(name) = &exp.name {
        assert_eq!(&rr.name, name, "{desc}: {label}.name");
    }
    if let Some(ttl) = exp.ttl {
        assert_eq!(rr.ttl, ttl, "{desc}: {label}.ttl");
    }

    match &rr.rdata {
        RData::A(ip) => {
            if let Some(rt) = &exp.rtype {
                assert_eq!(rt, "A", "{desc}: {label}.rtype");
            }
            if let Some(s) = &exp.ipv4 {
                assert_eq!(*ip, Ipv4Addr::from_str(s).unwrap(), "{desc}: {label}.ipv4");
            }
        }
        RData::AAAA(ip) => {
            if let Some(rt) = &exp.rtype {
                assert_eq!(rt, "AAAA", "{desc}: {label}.rtype");
            }
            if let Some(s) = &exp.ipv6 {
                assert_eq!(*ip, Ipv6Addr::from_str(s).unwrap(), "{desc}: {label}.ipv6");
            }
        }
        RData::CNAME(name) => {
            if let Some(rt) = &exp.rtype {
                assert_eq!(rt, "CNAME", "{desc}: {label}.rtype");
            }
            if let Some(s) = &exp.cname {
                assert_eq!(name, s, "{desc}: {label}.cname");
            }
        }
        RData::NS(name) => {
            if let Some(rt) = &exp.rtype {
                assert_eq!(rt, "NS", "{desc}: {label}.rtype");
            }
            if let Some(s) = &exp.ns {
                assert_eq!(name, s, "{desc}: {label}.ns");
            }
        }
        RData::PTR(name) => {
            if let Some(rt) = &exp.rtype {
                assert_eq!(rt, "PTR", "{desc}: {label}.rtype");
            }
            if let Some(s) = &exp.ptr {
                assert_eq!(name, s, "{desc}: {label}.ptr");
            }
        }
        RData::MX {
            preference,
            exchange,
        } => {
            if let Some(rt) = &exp.rtype {
                assert_eq!(rt, "MX", "{desc}: {label}.rtype");
            }
            if let Some(p) = exp.mx_pref {
                assert_eq!(*preference, p, "{desc}: {label}.mx_pref");
            }
            if let Some(e) = &exp.mx_exchange {
                assert_eq!(exchange, e, "{desc}: {label}.mx_exchange");
            }
        }
        RData::TXT(chunks) => {
            if let Some(rt) = &exp.rtype {
                assert_eq!(rt, "TXT", "{desc}: {label}.rtype");
            }
            if let Some(n) = exp.txt_chunks {
                assert_eq!(chunks.len(), n, "{desc}: {label}.txt_chunks");
            }
        }
        RData::SRV {
            priority,
            weight,
            port,
            target,
        } => {
            if let Some(rt) = &exp.rtype {
                assert_eq!(rt, "SRV", "{desc}: {label}.rtype");
            }
            if let Some(p) = exp.srv_priority {
                assert_eq!(*priority, p, "{desc}: {label}.srv_priority");
            }
            if let Some(w) = exp.srv_weight {
                assert_eq!(*weight, w, "{desc}: {label}.srv_weight");
            }
            if let Some(p) = exp.srv_port {
                assert_eq!(*port, p, "{desc}: {label}.srv_port");
            }
            if let Some(t) = &exp.srv_target {
                assert_eq!(target, t, "{desc}: {label}.srv_target");
            }
        }
        RData::SOA {
            mname,
            rname,
            serial,
            ..
        } => {
            if let Some(rt) = &exp.rtype {
                assert_eq!(rt, "SOA", "{desc}: {label}.rtype");
            }
            if let Some(m) = &exp.soa_mname {
                assert_eq!(mname, m, "{desc}: {label}.soa_mname");
            }
            if let Some(r) = &exp.soa_rname {
                assert_eq!(rname, r, "{desc}: {label}.soa_rname");
            }
            if let Some(s) = exp.soa_serial {
                assert_eq!(*serial, s, "{desc}: {label}.soa_serial");
            }
        }
        RData::OPT(_) | RData::Unknown { .. } => {}
    }
}

macro_rules! fixture_test {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            run_fixture(Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/",
                $file
            )));
        }
    };
}

fixture_test!(fixture_a_query, "a_query.toml");
fixture_test!(fixture_a_response, "a_response.toml");
fixture_test!(fixture_aaaa_response, "aaaa_response.toml");
fixture_test!(fixture_cname_response, "cname_response.toml");
fixture_test!(fixture_mx_response, "mx_response.toml");
fixture_test!(fixture_txt_response, "txt_response.toml");
fixture_test!(fixture_soa_response, "soa_response.toml");
fixture_test!(fixture_srv_response, "srv_response.toml");
fixture_test!(fixture_ptr_response, "ptr_response.toml");
fixture_test!(fixture_multi_answer, "multi_answer.toml");
fixture_test!(fixture_ns_response, "ns_response.toml");

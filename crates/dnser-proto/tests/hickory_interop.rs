// Interop tests: same wire bytes parsed by both dnser-proto and hickory-proto.
// Hex bytes are read from the shared TOML fixture files so there is no duplication.
//
// Two directions are tested for each message type:
//   1. Fixture hex → both parse → assert counts and ID agree
//   2. Our serialize → hickory parses → assert hickory accepts the output

use std::fs;

use bytes::Bytes;
use hickory_proto::op::Message as HickoryMessage;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    description: String,
    hex: String,
}

fn decode_hex(s: &str) -> Vec<u8> {
    let clean: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).unwrap())
        .collect()
}

fn check_interop(fixture_file: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture_file);
    let source = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let fixture: Fixture =
        toml::from_str(&source).unwrap_or_else(|e| panic!("parse TOML {path:?}: {e}"));

    let raw = decode_hex(&fixture.hex);
    let desc = &fixture.description;

    let ours = dnser_proto::Message::parse(Bytes::from(raw.clone()))
        .unwrap_or_else(|e| panic!("{desc}: our parse failed: {e}"));
    let hickory = HickoryMessage::from_vec(&raw)
        .unwrap_or_else(|e| panic!("{desc}: hickory parse failed: {e}"));

    assert_eq!(ours.header.id, hickory.id, "{desc}: id");
    assert_eq!(
        ours.questions.len(),
        hickory.queries.len(),
        "{desc}: question count"
    );
    assert_eq!(
        ours.answers.len(),
        hickory.answers.len(),
        "{desc}: answer count"
    );
    assert_eq!(
        ours.authority.len(),
        hickory.authorities.len(),
        "{desc}: authority count"
    );
    assert_eq!(
        ours.additional.len(),
        hickory.additionals.len(),
        "{desc}: additional count"
    );

    let our_bytes = ours
        .to_bytes()
        .unwrap_or_else(|e| panic!("{desc}: our to_bytes failed: {e}"));
    let reparsed = HickoryMessage::from_vec(&our_bytes)
        .unwrap_or_else(|e| panic!("{desc}: hickory rejected our serialized bytes: {e}"));
    assert_eq!(ours.header.id, reparsed.id, "{desc}: id after re-encode");
    assert_eq!(
        ours.answers.len(),
        reparsed.answers.len(),
        "{desc}: answer count after re-encode"
    );
}

macro_rules! interop_test {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            check_interop($file);
        }
    };
}

interop_test!(interop_a_query, "a_query.toml");
interop_test!(interop_a_response, "a_response.toml");
interop_test!(interop_aaaa_response, "aaaa_response.toml");
interop_test!(interop_cname_response, "cname_response.toml");
interop_test!(interop_mx_response, "mx_response.toml");
interop_test!(interop_txt_response, "txt_response.toml");
interop_test!(interop_soa_response, "soa_response.toml");
interop_test!(interop_srv_response, "srv_response.toml");
interop_test!(interop_ns_response, "ns_response.toml");
interop_test!(interop_ptr_response, "ptr_response.toml");
interop_test!(interop_multi_answer, "multi_answer.toml");

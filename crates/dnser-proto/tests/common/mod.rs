//! Shared helpers for the dnser-proto integration tests.
//!
//! Lives in a `common/mod.rs` rather than `common.rs` so cargo does not compile
//! it as its own test binary.

#![allow(dead_code)] // helpers used by a subset of the test files

/// Decode a hex string (ignoring whitespace and other non-hex bytes) into
/// raw bytes. Panics on odd-length input.
pub fn decode_hex(s: &str) -> Vec<u8> {
    let clean: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    assert!(clean.len() % 2 == 0, "odd-length hex string");
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).unwrap())
        .collect()
}

//! Shared async transport primitives for the dnser DNS server.

pub mod framing;

pub use framing::{read_framed, write_framed};

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedEof,
    InvalidPointer,
    PointerLoop,
    LabelTooLong,
    NameTooLong,
    RdataLengthMismatch,
    UnknownRecordType(u16),
    UnknownClass(u16),
    InvalidUtf8(std::str::Utf8Error),
    ReservedBitSet,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of buffer"),
            Self::InvalidPointer => write!(f, "compression pointer out of bounds"),
            Self::PointerLoop => write!(f, "compression pointer loop detected"),
            Self::LabelTooLong => write!(f, "label exceeds 63 bytes"),
            Self::NameTooLong => write!(f, "name exceeds 253 bytes"),
            Self::RdataLengthMismatch => write!(f, "rdata length mismatch"),
            Self::UnknownRecordType(t) => write!(f, "unknown record type: {t}"),
            Self::UnknownClass(c) => write!(f, "unknown class: {c}"),
            Self::InvalidUtf8(e) => write!(f, "invalid UTF-8 in label: {e}"),
            Self::ReservedBitSet => write!(f, "reserved Z bit is set"),
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    LabelTooLong,
    EmptyLabel,
    TxtChunkTooLong,
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LabelTooLong => write!(f, "label exceeds 63 bytes"),
            Self::EmptyLabel => write!(f, "domain name contains an empty label"),
            Self::TxtChunkTooLong => write!(f, "TXT chunk exceeds 255 bytes"),
        }
    }
}

impl std::error::Error for WriteError {}

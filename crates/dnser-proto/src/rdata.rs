use std::net::{Ipv4Addr, Ipv6Addr};

use bytes::Bytes;

use crate::error::{ParseError, WriteError};
use crate::reader::Reader;
use crate::record_type::RecordType;
use crate::writer::Writer;

/// A single EDNS(0) option (RFC 6891 §6.1.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdnsOption {
    pub code: u16,
    pub data: Bytes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RData {
    A(Ipv4Addr),
    AAAA(Ipv6Addr),
    NS(String),
    CNAME(String),
    SOA {
        mname: String,
        rname: String,
        serial: u32,
        refresh: u32,
        retry: u32,
        expire: u32,
        minimum: u32,
    },
    PTR(String),
    MX {
        preference: u16,
        exchange: String,
    },
    TXT(Vec<Bytes>),
    SRV {
        priority: u16,
        weight: u16,
        port: u16,
        target: String,
    },
    /// OPT pseudo-RR for EDNS(0) (RFC 6891). CLASS = UDP payload size; TTL = ext RCODE + version + flags.
    OPT(Vec<EdnsOption>),
    Unknown {
        rtype: u16,
        data: Bytes,
    },
}

impl RData {
    pub fn parse(r: &mut Reader, rtype: u16, rdlen: u16) -> Result<Self, ParseError> {
        let start = r.pos();
        let rdata = Self::parse_inner(r, rtype, rdlen)?;
        if r.pos() - start != rdlen as usize {
            return Err(ParseError::RdataLengthMismatch);
        }
        Ok(rdata)
    }

    fn parse_inner(r: &mut Reader, rtype: u16, rdlen: u16) -> Result<Self, ParseError> {
        match RecordType::try_from(rtype) {
            Ok(RecordType::A) => {
                if rdlen != 4 {
                    return Err(ParseError::RdataLengthMismatch);
                }
                let b = r.read_bytes(4)?;
                Ok(Self::A(Ipv4Addr::new(b[0], b[1], b[2], b[3])))
            }
            Ok(RecordType::AAAA) => {
                if rdlen != 16 {
                    return Err(ParseError::RdataLengthMismatch);
                }
                let b = r.read_bytes(16)?;
                Ok(Self::AAAA(Ipv6Addr::from(
                    <[u8; 16]>::try_from(b.as_ref()).unwrap(),
                )))
            }
            Ok(RecordType::NS) => Ok(Self::NS(r.read_name()?)),
            Ok(RecordType::CNAME) => Ok(Self::CNAME(r.read_name()?)),
            Ok(RecordType::PTR) => Ok(Self::PTR(r.read_name()?)),
            Ok(RecordType::MX) => Ok(Self::MX {
                preference: r.read_u16()?,
                exchange: r.read_name()?,
            }),
            Ok(RecordType::TXT) => {
                let mut chunks = Vec::new();
                let mut remaining = rdlen as usize;
                while remaining > 0 {
                    let len = r.read_u8()? as usize;
                    remaining = remaining
                        .checked_sub(1 + len)
                        .ok_or(ParseError::RdataLengthMismatch)?;
                    chunks.push(r.read_bytes(len)?);
                }
                Ok(Self::TXT(chunks))
            }
            Ok(RecordType::SOA) => Ok(Self::SOA {
                mname: r.read_name()?,
                rname: r.read_name()?,
                serial: r.read_u32()?,
                refresh: r.read_u32()?,
                retry: r.read_u32()?,
                expire: r.read_u32()?,
                minimum: r.read_u32()?,
            }),
            Ok(RecordType::SRV) => Ok(Self::SRV {
                priority: r.read_u16()?,
                weight: r.read_u16()?,
                port: r.read_u16()?,
                target: r.read_name()?,
            }),
            Ok(RecordType::OPT) => {
                let mut options = Vec::new();
                let mut remaining = rdlen as usize;
                while remaining > 0 {
                    let code = r.read_u16()?;
                    let len = r.read_u16()? as usize;
                    remaining = remaining
                        .checked_sub(4 + len)
                        .ok_or(ParseError::RdataLengthMismatch)?;
                    let data = r.read_bytes(len)?;
                    options.push(EdnsOption { code, data });
                }
                Ok(Self::OPT(options))
            }
            _ => Ok(Self::Unknown {
                rtype,
                data: r.read_bytes(rdlen as usize)?,
            }),
        }
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), WriteError> {
        match self {
            Self::A(addr) => w.write_bytes(&addr.octets()),
            Self::AAAA(addr) => w.write_bytes(&addr.octets()),
            Self::NS(name) | Self::CNAME(name) | Self::PTR(name) => w.write_name(name)?,
            Self::MX {
                preference,
                exchange,
            } => {
                w.write_u16(*preference);
                w.write_name(exchange)?;
            }
            Self::TXT(chunks) => {
                for chunk in chunks {
                    if chunk.len() > 255 {
                        return Err(WriteError::TxtChunkTooLong);
                    }
                    w.write_u8(chunk.len() as u8);
                    w.write_bytes(chunk);
                }
            }
            Self::SOA {
                mname,
                rname,
                serial,
                refresh,
                retry,
                expire,
                minimum,
            } => {
                w.write_name(mname)?;
                w.write_name(rname)?;
                w.write_u32(*serial);
                w.write_u32(*refresh);
                w.write_u32(*retry);
                w.write_u32(*expire);
                w.write_u32(*minimum);
            }
            Self::SRV {
                priority,
                weight,
                port,
                target,
            } => {
                w.write_u16(*priority);
                w.write_u16(*weight);
                w.write_u16(*port);
                w.write_name(target)?;
            }
            Self::OPT(options) => {
                for opt in options {
                    w.write_u16(opt.code);
                    w.write_u16(opt.data.len() as u16);
                    w.write_bytes(&opt.data);
                }
            }
            Self::Unknown { data, .. } => w.write_bytes(data),
        }
        Ok(())
    }
}

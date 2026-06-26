use crate::class::Class;
use crate::error::{ParseError, WriteError};
use crate::rdata::RData;
use crate::reader::Reader;
use crate::record_type::RecordType;
use crate::writer::Writer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRecord {
    pub name: String,
    pub class: Class,
    pub ttl: u32,
    pub rdata: RData,
}

impl ResourceRecord {
    /// Builds an EDNS(0) OPT pseudo-RR advertising `udp_size` (RFC 6891 §6.1.2).
    /// Name is root, class carries the requestor's UDP payload size, TTL is zero
    /// (extended RCODE=0, version=0, Z=0), and the RDATA carries no options.
    #[must_use]
    pub fn edns_opt(udp_size: u16) -> Self {
        Self {
            name: String::new(),
            class: Class::from(udp_size),
            ttl: 0,
            rdata: RData::OPT(Vec::new()),
        }
    }

    fn record_type(&self) -> Result<RecordType, u16> {
        match &self.rdata {
            RData::A(_) => Ok(RecordType::A),
            RData::AAAA(_) => Ok(RecordType::AAAA),
            RData::NS(_) => Ok(RecordType::NS),
            RData::CNAME(_) => Ok(RecordType::CNAME),
            RData::SOA { .. } => Ok(RecordType::SOA),
            RData::PTR(_) => Ok(RecordType::PTR),
            RData::MX { .. } => Ok(RecordType::MX),
            RData::TXT(_) => Ok(RecordType::TXT),
            RData::SRV { .. } => Ok(RecordType::SRV),
            RData::OPT(_) => Ok(RecordType::OPT),
            RData::Unknown { rtype, .. } => Err(*rtype),
        }
    }

    pub fn parse(r: &mut Reader) -> Result<Self, ParseError> {
        let name = r.read_name()?;
        let rtype = r.read_u16()?;
        let class = Class::from(r.read_u16()?);
        let ttl = r.read_u32()?;
        let rdlen = r.read_u16()?;
        let rdata = RData::parse(r, rtype, rdlen)?;
        Ok(Self {
            name,
            class,
            ttl,
            rdata,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), WriteError> {
        w.write_name(&self.name)?;
        match self.record_type() {
            Ok(rt) => w.write_u16(rt as u16),
            Err(n) => w.write_u16(n),
        }
        w.write_u16(u16::from(self.class));
        w.write_u32(self.ttl);
        let rdlen_pos = w.reserve_u16();
        let rdata_start = w.pos();
        self.rdata.write(w)?;
        let rdlen = (w.pos() - rdata_start) as u16;
        w.backfill_u16(rdlen_pos, rdlen);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use bytes::Bytes;

    use super::*;

    fn record(rdata: RData) -> ResourceRecord {
        ResourceRecord {
            name: "example.com".to_string(),
            class: Class::IN,
            ttl: 300,
            rdata,
        }
    }

    #[test]
    fn record_type_from_rdata() {
        assert_eq!(
            record(RData::A(Ipv4Addr::LOCALHOST)).record_type(),
            Ok(RecordType::A)
        );
        assert_eq!(
            record(RData::CNAME("alias.example.com".to_string())).record_type(),
            Ok(RecordType::CNAME)
        );
        assert_eq!(
            record(RData::MX {
                preference: 10,
                exchange: "mail.example.com".to_string()
            })
            .record_type(),
            Ok(RecordType::MX)
        );
    }

    #[test]
    fn edns_opt_has_expected_shape() {
        let opt = ResourceRecord::edns_opt(1232);
        assert!(opt.name.is_empty());
        assert_eq!(u16::from(opt.class), 1232);
        assert_eq!(opt.ttl, 0);
        assert_eq!(opt.rdata, RData::OPT(Vec::new()));
        assert_eq!(opt.record_type(), Ok(RecordType::OPT));
    }

    #[test]
    fn unknown_rdata_returns_err() {
        assert_eq!(
            record(RData::Unknown {
                rtype: 99,
                data: Bytes::new(),
            })
            .record_type(),
            Err(99)
        );
    }
}

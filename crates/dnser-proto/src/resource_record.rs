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

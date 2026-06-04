use crate::class::Class;
use crate::rdata::RData;
use crate::record_type::RecordType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRecord {
    pub name: String,
    pub class: Class,
    pub ttl: u32,
    pub rdata: RData,
}

impl ResourceRecord {
    pub fn record_type(&self) -> Result<RecordType, u16> {
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
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

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
                data: vec![]
            })
            .record_type(),
            Err(99)
        );
    }
}

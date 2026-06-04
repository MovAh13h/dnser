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

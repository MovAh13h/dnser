use crate::class::Class;
use crate::error::{ParseError, WriteError};
use crate::reader::Reader;
use crate::record_type::RecordType;
use crate::writer::Writer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub name: String,
    pub qtype: RecordType,
    pub qclass: Class,
}

impl Question {
    pub fn parse(r: &mut Reader) -> Result<Self, ParseError> {
        let name = r.read_name()?;
        let qtype = RecordType::try_from(r.read_u16()?).map_err(ParseError::UnknownRecordType)?;
        let qclass = Class::try_from(r.read_u16()?).map_err(ParseError::UnknownClass)?;
        Ok(Self {
            name,
            qtype,
            qclass,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), WriteError> {
        w.write_name(&self.name)?;
        w.write_u16(self.qtype as u16);
        w.write_u16(self.qclass as u16);
        Ok(())
    }
}

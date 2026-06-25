use bytes::{Bytes, BytesMut};

use crate::MAX_UDP_SIZE;
use crate::error::{ParseError, WriteError};
use crate::header::Header;
use crate::question::Question;
use crate::reader::Reader;
use crate::resource_record::ResourceRecord;
use crate::writer::Writer;

// Minimum wire size of a resource record (2-byte compressed name + type + class + ttl + rdlen).
const MIN_RR_BYTES: usize = 12;
// Hard cap on section capacity to prevent large allocations from malformed counts.
const MAX_SECTION_CAP: usize = MAX_UDP_SIZE / MIN_RR_BYTES;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Message {
    pub header: Header,
    pub questions: Vec<Question>,
    pub answers: Vec<ResourceRecord>,
    pub authority: Vec<ResourceRecord>,
    pub additional: Vec<ResourceRecord>,
}

fn parse_section<T, F>(r: &mut Reader, count: u16, parse: F) -> Result<Vec<T>, ParseError>
where
    F: Fn(&mut Reader) -> Result<T, ParseError>,
{
    let mut v = Vec::with_capacity((count as usize).min(MAX_SECTION_CAP));
    for _ in 0..count {
        v.push(parse(r)?);
    }
    Ok(v)
}

impl Message {
    pub fn parse(buf: Bytes) -> Result<Self, ParseError> {
        let mut r = Reader::new(buf);
        let header = Header::parse(&mut r)?;
        let questions = parse_section(&mut r, header.qd_count, Question::parse)?;
        let answers = parse_section(&mut r, header.an_count, ResourceRecord::parse)?;
        let authority = parse_section(&mut r, header.ns_count, ResourceRecord::parse)?;
        let additional = parse_section(&mut r, header.ar_count, ResourceRecord::parse)?;
        Ok(Self {
            header,
            questions,
            answers,
            authority,
            additional,
        })
    }

    fn build_writer(&self) -> Result<Writer, WriteError> {
        let mut w = Writer::new();
        self.header.write(&mut w);
        for q in &self.questions {
            q.write(&mut w)?;
        }
        for rr in &self.answers {
            rr.write(&mut w)?;
        }
        for rr in &self.authority {
            rr.write(&mut w)?;
        }
        for rr in &self.additional {
            rr.write(&mut w)?;
        }
        Ok(w)
    }

    pub fn to_bytes(&self) -> Result<Bytes, WriteError> {
        Ok(self.build_writer()?.finish())
    }

    /// Serializes the message into a `BytesMut`, useful when the caller needs
    /// to patch bytes in place (e.g. rewriting the query ID) before sending.
    pub fn to_bytes_mut(&self) -> Result<BytesMut, WriteError> {
        Ok(self.build_writer()?.into_inner())
    }
}

impl TryFrom<Bytes> for Message {
    type Error = ParseError;

    fn try_from(buf: Bytes) -> Result<Self, Self::Error> {
        Self::parse(buf)
    }
}

impl TryFrom<&[u8]> for Message {
    type Error = ParseError;

    fn try_from(buf: &[u8]) -> Result<Self, Self::Error> {
        Self::parse(Bytes::copy_from_slice(buf))
    }
}

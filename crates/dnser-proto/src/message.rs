use bytes::Bytes;

use crate::error::{ParseError, WriteError};
use crate::header::Header;
use crate::question::Question;
use crate::reader::Reader;
use crate::resource_record::ResourceRecord;
use crate::writer::Writer;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Message {
    pub header: Header,
    pub questions: Vec<Question>,
    pub answers: Vec<ResourceRecord>,
    pub authority: Vec<ResourceRecord>,
    pub additional: Vec<ResourceRecord>,
}

impl Message {
    pub fn parse(buf: Bytes) -> Result<Self, ParseError> {
        let mut r = Reader::new(buf);
        let header = Header::parse(&mut r)?;

        // TODO: cap with_capacity to a safe max once dnser-server clarifies expected packet sizes.
        // Untrusted counts (e.g. an_count=65535 on a 12-byte packet) cause large allocations
        // before hitting UnexpectedEof. Correctness is unaffected; this is hygiene only.
        let mut questions = Vec::with_capacity(header.qd_count as usize);
        for _ in 0..header.qd_count {
            questions.push(Question::parse(&mut r)?);
        }

        let mut answers = Vec::with_capacity(header.an_count as usize);
        for _ in 0..header.an_count {
            answers.push(ResourceRecord::parse(&mut r)?);
        }

        let mut authority = Vec::with_capacity(header.ns_count as usize);
        for _ in 0..header.ns_count {
            authority.push(ResourceRecord::parse(&mut r)?);
        }

        let mut additional = Vec::with_capacity(header.ar_count as usize);
        for _ in 0..header.ar_count {
            additional.push(ResourceRecord::parse(&mut r)?);
        }

        Ok(Self {
            header,
            questions,
            answers,
            authority,
            additional,
        })
    }

    pub fn to_bytes(&self) -> Result<Bytes, WriteError> {
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
        Ok(w.finish())
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

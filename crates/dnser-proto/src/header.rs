use crate::error::ParseError;
use crate::opcode::Opcode;
use crate::rcode::Rcode;
use crate::reader::Reader;
use crate::writer::Writer;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub id: u16,
    pub flags: u16,
    pub qd_count: u16,
    pub an_count: u16,
    pub ns_count: u16,
    pub ar_count: u16,
}

impl Header {
    pub const QR: u16 = 0b1000_0000_0000_0000;
    pub const OPCODE: u16 = 0b0111_1000_0000_0000;
    pub const AA: u16 = 0b0000_0100_0000_0000;
    pub const TC: u16 = 0b0000_0010_0000_0000;
    pub const RD: u16 = 0b0000_0001_0000_0000;
    pub const RA: u16 = 0b0000_0000_1000_0000;
    pub const Z: u16 = 0b0000_0000_0100_0000;
    pub const AD: u16 = 0b0000_0000_0010_0000;
    pub const CD: u16 = 0b0000_0000_0001_0000;
    pub const RCODE: u16 = 0b0000_0000_0000_1111;

    pub fn is_response(&self) -> bool {
        self.flags & Self::QR != 0
    }

    pub fn is_query(&self) -> bool {
        self.flags & Self::QR == 0
    }

    pub fn opcode(&self) -> Result<Opcode, u8> {
        Opcode::try_from(((self.flags & Self::OPCODE) >> 11) as u8)
    }

    pub fn is_authoritative(&self) -> bool {
        self.flags & Self::AA != 0
    }

    pub fn is_truncated(&self) -> bool {
        self.flags & Self::TC != 0
    }

    pub fn recursion_desired(&self) -> bool {
        self.flags & Self::RD != 0
    }

    pub fn recursion_available(&self) -> bool {
        self.flags & Self::RA != 0
    }

    pub fn authentic_data(&self) -> bool {
        self.flags & Self::AD != 0
    }

    pub fn checking_disabled(&self) -> bool {
        self.flags & Self::CD != 0
    }

    pub fn rcode(&self) -> Result<Rcode, u8> {
        Rcode::try_from((self.flags & Self::RCODE) as u8)
    }

    /// Builds a response header for `query`: same id, same qd_count, preserves
    /// the RD bit, sets QR plus any caller-supplied `extra_flags` (e.g. TC,
    /// an rcode, RA). The section counts default to zero — the caller fills
    /// `an_count`/`ns_count`/`ar_count` to match the sections it appends.
    #[must_use]
    pub fn reply_to(query: &Header, extra_flags: u16) -> Self {
        Self {
            id: query.id,
            flags: Self::QR | extra_flags | (query.flags & Self::RD),
            qd_count: query.qd_count,
            ..Default::default()
        }
    }

    pub fn parse(r: &mut Reader) -> Result<Self, ParseError> {
        let id = r.read_u16()?;
        let flags = r.read_u16()?;
        if flags & Self::Z != 0 {
            return Err(ParseError::ReservedBitSet);
        }
        Ok(Self {
            id,
            flags,
            qd_count: r.read_u16()?,
            an_count: r.read_u16()?,
            ns_count: r.read_u16()?,
            ar_count: r.read_u16()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.write_u16(self.id);
        w.write_u16(self.flags);
        w.write_u16(self.qd_count);
        w.write_u16(self.an_count);
        w.write_u16(self.ns_count);
        w.write_u16(self.ar_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(flags: u16) -> Header {
        Header {
            id: 0,
            flags,
            qd_count: 0,
            an_count: 0,
            ns_count: 0,
            ar_count: 0,
        }
    }

    #[test]
    fn qr_flag() {
        let response = header(0b1000_0000_0000_0000);
        let query = header(0b0000_0000_0000_0000);
        assert!(response.is_response() && !response.is_query());
        assert!(query.is_query() && !query.is_response());
    }

    #[test]
    fn opcode_flag() {
        let cases: &[(u16, Result<Opcode, u8>)] = &[
            (0b0000_0000_0000_0000, Ok(Opcode::Query)),
            (0b0000_1000_0000_0000, Ok(Opcode::IQuery)),
            (0b0010_0000_0000_0000, Ok(Opcode::Notify)),
            (0b0001_1000_0000_0000, Err(3)),
        ];
        for &(flags, expected) in cases {
            assert_eq!(header(flags).opcode(), expected);
        }
    }

    #[test]
    fn single_bit_flags() {
        type FlagAccessor = fn(&Header) -> bool;
        let cases: &[(u16, FlagAccessor)] = &[
            (0b0000_0100_0000_0000, Header::is_authoritative),
            (0b0000_0010_0000_0000, Header::is_truncated),
            (0b0000_0001_0000_0000, Header::recursion_desired),
            (0b0000_0000_1000_0000, Header::recursion_available),
            (0b0000_0000_0010_0000, Header::authentic_data),
            (0b0000_0000_0001_0000, Header::checking_disabled),
        ];
        for &(flags, accessor) in cases {
            assert!(accessor(&header(flags)));
        }
    }

    #[test]
    fn reply_to_sets_qr_copies_id_and_qd_count() {
        let mut query = header(0);
        query.id = 0xABCD;
        query.qd_count = 1;
        let resp = Header::reply_to(&query, 0);
        assert!(resp.is_response());
        assert_eq!(resp.id, 0xABCD);
        assert_eq!(resp.qd_count, 1);
    }

    #[test]
    fn reply_to_preserves_rd_bit() {
        assert!(Header::reply_to(&header(Header::RD), 0).recursion_desired());
        assert!(!Header::reply_to(&header(0), 0).recursion_desired());
    }

    #[test]
    fn reply_to_ors_extra_flags() {
        let resp = Header::reply_to(&header(Header::RD), Header::TC | Header::RA);
        assert!(resp.is_truncated());
        assert!(resp.recursion_available());
        assert!(resp.recursion_desired());
    }

    #[test]
    fn reply_to_does_not_leak_other_query_flags() {
        // CD on the query must NOT appear on the reply — only RD is forwarded.
        let resp = Header::reply_to(&header(Header::CD | Header::RD), 0);
        assert!(!resp.checking_disabled());
        assert!(resp.recursion_desired());
    }

    #[test]
    fn rcode_flag() {
        let cases: &[(u16, Result<Rcode, u8>)] = &[
            (0b0000_0000_0000_0000, Ok(Rcode::NoError)),
            (0b0000_0000_0000_0011, Ok(Rcode::NXDomain)),
            (0b0000_0000_0000_0101, Ok(Rcode::Refused)),
            (0b0000_0000_0000_0110, Err(6)),
        ];
        for &(flags, expected) in cases {
            assert_eq!(header(flags).rcode(), expected);
        }
    }
}

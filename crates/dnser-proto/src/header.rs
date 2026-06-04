use crate::opcode::Opcode;
use crate::rcode::Rcode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub id: u16,
    pub flags: u16,
    pub qd_count: u16,
    pub an_count: u16,
    pub ns_count: u16,
    pub ar_count: u16,
}

impl Header {
    const QR: u16 = 0b1000_0000_0000_0000;
    const OPCODE: u16 = 0b0111_1000_0000_0000;
    const AA: u16 = 0b0000_0100_0000_0000;
    const TC: u16 = 0b0000_0010_0000_0000;
    const RD: u16 = 0b0000_0001_0000_0000;
    const RA: u16 = 0b0000_0000_1000_0000;
    const AD: u16 = 0b0000_0000_0010_0000;
    const CD: u16 = 0b0000_0000_0001_0000;
    const RCODE: u16 = 0b0000_0000_0000_1111;

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
}

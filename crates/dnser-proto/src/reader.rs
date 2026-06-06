use bytes::Bytes;

use crate::error::ParseError;

pub struct Reader {
    buf: Bytes,
    pos: usize,
}

impl Reader {
    pub fn new(buf: Bytes) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn read_u8(&mut self) -> Result<u8, ParseError> {
        self.buf
            .get(self.pos)
            .copied()
            .ok_or(ParseError::UnexpectedEof)
            .inspect(|_| self.pos += 1)
    }

    pub fn read_u16(&mut self) -> Result<u16, ParseError> {
        let s = self
            .buf
            .get(self.pos..self.pos + 2)
            .ok_or(ParseError::UnexpectedEof)?;
        let v = u16::from_be_bytes(<[u8; 2]>::try_from(s).unwrap());
        self.pos += 2;
        Ok(v)
    }

    pub fn read_u32(&mut self) -> Result<u32, ParseError> {
        let s = self
            .buf
            .get(self.pos..self.pos + 4)
            .ok_or(ParseError::UnexpectedEof)?;
        let v = u32::from_be_bytes(<[u8; 4]>::try_from(s).unwrap());
        self.pos += 4;
        Ok(v)
    }

    pub fn read_bytes(&mut self, n: usize) -> Result<Bytes, ParseError> {
        if self.pos + n > self.buf.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let slice = self.buf.slice(self.pos..self.pos + n);
        self.pos += n;
        Ok(slice)
    }

    pub fn read_name(&mut self) -> Result<String, ParseError> {
        const MAX_JUMPS: usize = 128;
        const MAX_NAME_LEN: usize = 253;

        let mut name = String::with_capacity(64);
        let mut pos = self.pos;
        let mut jumped = false;
        let mut jumps = 0;

        loop {
            if jumps >= MAX_JUMPS {
                return Err(ParseError::PointerLoop);
            }
            if pos >= self.buf.len() {
                return Err(ParseError::UnexpectedEof);
            }

            let byte = self.buf[pos];

            if byte == 0 {
                if !jumped {
                    self.pos = pos + 1;
                }
                break;
            } else if byte & 0b1100_0000 == 0b1100_0000 {
                if pos + 1 >= self.buf.len() {
                    return Err(ParseError::UnexpectedEof);
                }
                let offset = (((byte & 0b0011_1111) as usize) << 8) | self.buf[pos + 1] as usize;
                if offset >= self.buf.len() {
                    return Err(ParseError::InvalidPointer);
                }
                if !jumped {
                    self.pos = pos + 2;
                }
                pos = offset;
                jumped = true;
                jumps += 1;
            } else {
                let len = byte as usize;
                if len > 63 {
                    return Err(ParseError::LabelTooLong);
                }
                pos += 1;
                if pos + len > self.buf.len() {
                    return Err(ParseError::UnexpectedEof);
                }
                if !name.is_empty() {
                    name.push('.');
                }
                let label = std::str::from_utf8(&self.buf[pos..pos + len])
                    .map_err(ParseError::InvalidUtf8)?;
                name.push_str(label);
                if name.len() > MAX_NAME_LEN {
                    return Err(ParseError::NameTooLong);
                }
                pos += len;
            }
        }

        Ok(name)
    }
}

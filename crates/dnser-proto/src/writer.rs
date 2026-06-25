use std::collections::HashMap;

use bytes::{BufMut, Bytes, BytesMut};

use crate::error::WriteError;

pub struct Writer {
    buf: BytesMut,
    name_offsets: HashMap<String, u16>,
}

impl Writer {
    pub fn new() -> Self {
        Self {
            buf: BytesMut::with_capacity(512),
            name_offsets: HashMap::new(),
        }
    }

    pub fn pos(&self) -> usize {
        self.buf.len()
    }

    pub fn write_u8(&mut self, v: u8) {
        self.buf.put_u8(v);
    }

    pub fn write_u16(&mut self, v: u16) {
        self.buf.put_u16(v);
    }

    pub fn write_u32(&mut self, v: u32) {
        self.buf.put_u32(v);
    }

    pub fn write_bytes(&mut self, b: &[u8]) {
        self.buf.put_slice(b);
    }

    pub fn reserve_u16(&mut self) -> usize {
        let pos = self.buf.len();
        self.buf.put_u16(0);
        pos
    }

    pub fn backfill_u16(&mut self, pos: usize, v: u16) {
        let bytes = v.to_be_bytes();
        self.buf[pos] = bytes[0];
        self.buf[pos + 1] = bytes[1];
    }

    pub fn write_name(&mut self, name: &str) -> Result<(), WriteError> {
        let name = name.trim_end_matches('.');

        if name.is_empty() {
            self.write_u8(0);
            return Ok(());
        }

        let mut offset = 0usize;

        for label in name.split('.') {
            let suffix = &name[offset..];

            if let Some(&ptr) = self.name_offsets.get(suffix) {
                self.write_u16(0b1100_0000_0000_0000u16 | ptr);
                return Ok(());
            }

            let current_pos = self.buf.len();
            if current_pos <= 0b0011_1111_1111_1111 {
                self.name_offsets
                    .insert(suffix.to_string(), current_pos as u16);
            }

            let label = label.as_bytes();
            if label.is_empty() {
                return Err(WriteError::EmptyLabel);
            }
            if label.len() > 63 {
                return Err(WriteError::LabelTooLong);
            }
            self.write_u8(label.len() as u8);
            self.write_bytes(label);

            offset += label.len() + 1;
        }

        self.write_u8(0);
        Ok(())
    }

    pub fn finish(self) -> Bytes {
        self.buf.freeze()
    }

    pub fn into_inner(self) -> BytesMut {
        self.buf
    }
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

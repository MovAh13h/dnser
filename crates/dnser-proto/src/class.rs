#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Class(u16);

impl Class {
    pub const IN: Self = Self(1);
    pub const CS: Self = Self(2);
    pub const CH: Self = Self(3);
    pub const HS: Self = Self(4);
    pub const ANY: Self = Self(255);

    pub fn is_known(self) -> bool {
        matches!(self.0, 1 | 2 | 3 | 4 | 255)
    }
}

impl From<u16> for Class {
    fn from(v: u16) -> Self {
        Self(v)
    }
}

impl From<Class> for u16 {
    fn from(c: Class) -> Self {
        c.0
    }
}

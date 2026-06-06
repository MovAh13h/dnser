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
        self == Self::IN
            || self == Self::CS
            || self == Self::CH
            || self == Self::HS
            || self == Self::ANY
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

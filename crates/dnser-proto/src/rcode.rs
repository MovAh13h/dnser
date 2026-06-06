#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Rcode {
    NoError = 0,
    FormErr = 1,
    ServFail = 2,
    NXDomain = 3,
    NotImp = 4,
    Refused = 5,
}

impl TryFrom<u8> for Rcode {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::NoError),
            1 => Ok(Self::FormErr),
            2 => Ok(Self::ServFail),
            3 => Ok(Self::NXDomain),
            4 => Ok(Self::NotImp),
            5 => Ok(Self::Refused),
            n => Err(n),
        }
    }
}

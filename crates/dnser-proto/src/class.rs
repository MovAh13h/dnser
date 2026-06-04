#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Class {
	IN  = 1,
	CS  = 2,
	CH  = 3,
	HS  = 4,
	ANY = 255,
}

impl TryFrom<u16> for Class {
	type Error = u16;

	fn try_from(value: u16) -> Result<Self, Self::Error> {
		match value {
			1   => Ok(Self::IN),
			2   => Ok(Self::CS),
			3   => Ok(Self::CH),
			4   => Ok(Self::HS),
			255 => Ok(Self::ANY),
			n   => Err(n),
		}
	}
}

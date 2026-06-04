#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum RecordType {
	A     = 1,
	NS    = 2,
	CNAME = 5,
	SOA   = 6,
	PTR   = 12,
	MX    = 15,
	TXT   = 16,
	AAAA  = 28,
	SRV   = 33,
	IXFR  = 251,
	AXFR  = 252,
	ANY   = 255,
}

impl TryFrom<u16> for RecordType {
	type Error = u16;

	fn try_from(value: u16) -> Result<Self, Self::Error> {
		match value {
			1   => Ok(Self::A),
			2   => Ok(Self::NS),
			5   => Ok(Self::CNAME),
			6   => Ok(Self::SOA),
			12  => Ok(Self::PTR),
			15  => Ok(Self::MX),
			16  => Ok(Self::TXT),
			28  => Ok(Self::AAAA),
			33  => Ok(Self::SRV),
			251 => Ok(Self::IXFR),
			252 => Ok(Self::AXFR),
			255 => Ok(Self::ANY),
			n   => Err(n),
		}
	}
}
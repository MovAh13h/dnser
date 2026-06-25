use std::fmt;

#[derive(Debug)]
pub enum ResolveError {
    Write(dnser_proto::WriteError),
    Parse(dnser_proto::ParseError),
    Io(std::io::Error),
    Timeout,
    NoUpstreams,
    AllFailed,
    InvalidResponse,
    IdSpaceExhausted,
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Write(e) => write!(f, "write: {e}"),
            Self::Parse(e) => write!(f, "parse: {e}"),
            Self::Io(e) => write!(f, "io: {e}"),
            Self::Timeout => write!(f, "upstream timed out"),
            Self::NoUpstreams => write!(f, "no upstreams configured"),
            Self::AllFailed => write!(f, "all upstreams failed"),
            Self::InvalidResponse => write!(f, "invalid response from upstream"),
            Self::IdSpaceExhausted => write!(f, "all 65536 query IDs in use"),
        }
    }
}

impl std::error::Error for ResolveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Write(e) => Some(e),
            Self::Parse(e) => Some(e),
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<dnser_proto::WriteError> for ResolveError {
    fn from(e: dnser_proto::WriteError) -> Self {
        Self::Write(e)
    }
}

impl From<dnser_proto::ParseError> for ResolveError {
    fn from(e: dnser_proto::ParseError) -> Self {
        Self::Parse(e)
    }
}

impl From<std::io::Error> for ResolveError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

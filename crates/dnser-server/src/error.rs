use std::fmt;

#[derive(Debug)]
pub(crate) enum QueryError {
    Parse(dnser_proto::ParseError),
    Write(dnser_proto::WriteError),
    Io(std::io::Error),
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "parse: {e}"),
            Self::Write(e) => write!(f, "write: {e}"),
            Self::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl From<dnser_proto::ParseError> for QueryError {
    fn from(e: dnser_proto::ParseError) -> Self {
        Self::Parse(e)
    }
}

impl From<dnser_proto::WriteError> for QueryError {
    fn from(e: dnser_proto::WriteError) -> Self {
        Self::Write(e)
    }
}

impl From<std::io::Error> for QueryError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

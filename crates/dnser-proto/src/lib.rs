/// Maximum DNS message size over UDP.
pub const MAX_UDP_SIZE: usize = 4096;

pub mod class;
pub mod error;
pub mod header;
pub mod message;
pub mod opcode;
pub mod question;
pub mod rcode;
pub mod rdata;
pub mod reader;
pub mod record_type;
pub mod resource_record;
pub mod writer;

pub use class::Class;
pub use error::{ParseError, WriteError};
pub use header::Header;
pub use message::Message;
pub use opcode::Opcode;
pub use question::Question;
pub use rcode::Rcode;
pub use rdata::{EdnsOption, RData};
pub use reader::Reader;
pub use record_type::RecordType;
pub use resource_record::ResourceRecord;
pub use writer::Writer;

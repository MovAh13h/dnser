use crate::class::Class;
use crate::record_type::RecordType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub name: String,
    pub qtype: RecordType,
    pub qclass: Class,
}

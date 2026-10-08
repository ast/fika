use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtoError {
    #[error("CRC mismatch")]
    Crc,
    #[error("unsupported protocol version {0}")]
    Version(u8),
    #[error("unknown frame type {0}")]
    FrameType(u8),
    #[error("block has {0} bytes, expected {1}")]
    BlockLength(usize, usize),
    #[error("callsign '{0}' cannot be packed")]
    Callsign(String),
    #[error("text decoding failed: {0}")]
    Text(&'static str),
    #[error("message needs {0} bits, more than the {1} bit maximum")]
    TooLong(usize, usize),
    #[error("block {0} is missing")]
    MissingBlock(usize),
    #[error("invalid field value: {0}")]
    Field(&'static str),
}

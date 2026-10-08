//! fika link layer: bit packing, CRC, callsigns, groups, frame headers,
//! text coding and message assembly. See `docs/SPEC.md` §7, §9.

pub mod bits;
pub mod callsign;
pub mod crc16;
pub mod error;
pub mod frame;
pub mod group;
pub mod message;
pub mod prior;
pub mod text;

pub use callsign::Callsign;
pub use error::ProtoError;
pub use frame::{
    Ack, Beacon, Block0, Continuation, Destination, Frame, FrameType, PROTOCOL_VERSION,
};
pub use message::{Message, MessageAssembler};

//! Frame layouts. Every frame is exactly one GF(64) LDPC info block of
//! 48 bytes (384 bits); short frames (ACK, beacon) pad the rest.

use crate::bits::{BitReader, BitWriter, bytes_to_bits};
use crate::crc16::crc16;
use crate::error::ProtoError;

pub const PROTOCOL_VERSION: u8 = 0;
pub const LONG_BLOCK_BYTES: usize = 48;
pub const SHORT_BLOCK_BYTES: usize = 48;
/// Payload bits in block 0 and in each continuation block.
pub const BLOCK0_PAYLOAD_BITS: usize = 282;
pub const CONT_PAYLOAD_BITS: usize = 349;
pub const MAX_BLOCKS: usize = 8;

pub fn payload_capacity(blocks: usize) -> usize {
    BLOCK0_PAYLOAD_BITS + CONT_PAYLOAD_BITS * blocks.saturating_sub(1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameType {
    Message = 0,
    Ack = 1,
    Beacon = 2,
}

impl TryFrom<u8> for FrameType {
    type Error = ProtoError;
    fn try_from(v: u8) -> Result<Self, ProtoError> {
        match v {
            0 => Ok(FrameType::Message),
            1 => Ok(FrameType::Ack),
            2 => Ok(FrameType::Beacon),
            other => Err(ProtoError::FrameType(other)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Destination {
    All,
    Group(u32),
    Call(u32),
}

impl Destination {
    fn type_bits(self) -> u64 {
        match self {
            Destination::All => 0,
            Destination::Group(_) => 1,
            Destination::Call(_) => 2,
        }
    }
    fn value_bits(self) -> u64 {
        match self {
            Destination::All => 0,
            Destination::Group(g) | Destination::Call(g) => g as u64,
        }
    }
    fn from_bits(t: u64, v: u64) -> Result<Self, ProtoError> {
        match t {
            0 => Ok(Destination::All),
            1 => Ok(Destination::Group(v as u32)),
            2 => Ok(Destination::Call(v as u32)),
            _ => Err(ProtoError::Field("destination type")),
        }
    }
}

/// Block 0 of a message burst.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block0 {
    pub sender: u32,
    pub dest: Destination,
    pub msg_id: u16,
    /// Number of blocks in the burst, 1..=8.
    pub total: u8,
    pub hop: u8,
    pub ack_req: bool,
    pub raw_text: bool,
    /// Exactly 282 payload bits.
    pub payload: Vec<u8>,
}

/// Blocks 1..7 of a message burst.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Continuation {
    pub seq: u8,
    pub msg_id: u16,
    /// Exactly 349 payload bits.
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ack {
    pub sender: u32,
    pub dest: u32,
    pub msg_id: u16,
    pub snr_db: i8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Beacon {
    pub sender: u32,
    /// 15-bit grid index or None.
    pub grid: Option<u16>,
    pub group_tags: [u16; 2],
    pub status: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    Block0(Block0),
    Ack(Ack),
    Beacon(Beacon),
}

fn finish(w: &mut BitWriter, total_bits: usize) -> Vec<u8> {
    assert_eq!(w.len(), total_bits - 16, "layout error");
    let crc = crc16(&w.to_bytes());
    w.push(crc as u64, 16);
    w.to_bytes()
}

fn check(bytes: &[u8], expected_len: usize) -> Result<Vec<u8>, ProtoError> {
    if bytes.len() != expected_len {
        return Err(ProtoError::BlockLength(bytes.len(), expected_len));
    }
    let body = &bytes[..expected_len - 2];
    let crc = u16::from_be_bytes([bytes[expected_len - 2], bytes[expected_len - 1]]);
    if crc16(body) != crc {
        return Err(ProtoError::Crc);
    }
    Ok(bytes_to_bits(body))
}

impl Block0 {
    pub fn pack(&self) -> Vec<u8> {
        assert_eq!(self.payload.len(), BLOCK0_PAYLOAD_BITS);
        let mut w = BitWriter::new();
        w.push(PROTOCOL_VERSION as u64, 2);
        w.push(FrameType::Message as u64, 3);
        w.push(self.sender as u64, 28);
        w.push(self.dest.type_bits(), 2);
        w.push(self.dest.value_bits(), 28);
        w.push(self.msg_id as u64, 16);
        w.push((self.total - 1) as u64, 3);
        w.push(self.hop as u64, 2);
        w.push(((self.ack_req as u64) << 1) | self.raw_text as u64, 2);
        w.push_bits(&self.payload);
        finish(&mut w, LONG_BLOCK_BYTES * 8)
    }
}

impl Continuation {
    pub fn pack(&self) -> Vec<u8> {
        assert_eq!(self.payload.len(), CONT_PAYLOAD_BITS);
        let mut w = BitWriter::new();
        w.push(self.seq as u64, 3);
        w.push(self.msg_id as u64, 16);
        w.push_bits(&self.payload);
        finish(&mut w, LONG_BLOCK_BYTES * 8)
    }

    /// Continuation blocks carry no type field; the caller knows from
    /// timing that this is block `seq` of a burst.
    pub fn parse(bytes: &[u8]) -> Result<Self, ProtoError> {
        let bits = check(bytes, LONG_BLOCK_BYTES)?;
        let mut r = BitReader::new(&bits);
        let seq = r.read(3) as u8;
        let msg_id = r.read(16) as u16;
        let payload = r.read_bits(CONT_PAYLOAD_BITS).to_vec();
        if seq == 0 {
            return Err(ProtoError::Field("continuation seq 0"));
        }
        Ok(Self {
            seq,
            msg_id,
            payload,
        })
    }
}

impl Ack {
    pub fn pack(&self) -> Vec<u8> {
        let mut w = BitWriter::new();
        w.push(PROTOCOL_VERSION as u64, 2);
        w.push(FrameType::Ack as u64, 3);
        w.push(self.sender as u64, 28);
        w.push(self.dest as u64, 28);
        w.push(self.msg_id as u64, 16);
        w.push((self.snr_db.clamp(-32, 31) as i64 & 0x3F) as u64, 6);
        w.pad_to(SHORT_BLOCK_BYTES * 8 - 16);
        finish(&mut w, SHORT_BLOCK_BYTES * 8)
    }
}

impl Beacon {
    pub fn pack(&self) -> Vec<u8> {
        let mut w = BitWriter::new();
        w.push(PROTOCOL_VERSION as u64, 2);
        w.push(FrameType::Beacon as u64, 3);
        w.push(self.sender as u64, 28);
        w.push(self.grid.unwrap_or(0x7FFF) as u64, 15);
        w.push(self.group_tags[0] as u64, 12);
        w.push(self.group_tags[1] as u64, 12);
        w.push(self.status as u64, 4);
        w.pad_to(SHORT_BLOCK_BYTES * 8 - 16);
        finish(&mut w, SHORT_BLOCK_BYTES * 8)
    }
}

impl Frame {
    /// Parse block 0 of a long frame or a whole short frame.
    pub fn parse(bytes: &[u8]) -> Result<Frame, ProtoError> {
        let bits = check(bytes, LONG_BLOCK_BYTES)?;
        let mut r = BitReader::new(&bits);
        let ver = r.read(2) as u8;
        if ver != PROTOCOL_VERSION {
            return Err(ProtoError::Version(ver));
        }
        let ty = FrameType::try_from(r.read(3) as u8)?;
        match ty {
            FrameType::Message => {
                let sender = r.read(28) as u32;
                let dt = r.read(2);
                let dv = r.read(28);
                let dest = Destination::from_bits(dt, dv)?;
                let msg_id = r.read(16) as u16;
                let total = r.read(3) as u8 + 1;
                let hop = r.read(2) as u8;
                let flags = r.read(2);
                let payload = r.read_bits(BLOCK0_PAYLOAD_BITS).to_vec();
                Ok(Frame::Block0(Block0 {
                    sender,
                    dest,
                    msg_id,
                    total,
                    hop,
                    ack_req: flags & 2 != 0,
                    raw_text: flags & 1 != 0,
                    payload,
                }))
            }
            FrameType::Ack => {
                let sender = r.read(28) as u32;
                let dest = r.read(28) as u32;
                let msg_id = r.read(16) as u16;
                let raw = r.read(6) as u8;
                let snr_db = ((raw << 2) as i8) >> 2;
                Ok(Frame::Ack(Ack {
                    sender,
                    dest,
                    msg_id,
                    snr_db,
                }))
            }
            FrameType::Beacon => {
                let sender = r.read(28) as u32;
                let grid = r.read(15) as u16;
                let group_tags = [r.read(12) as u16, r.read(12) as u16];
                let status = r.read(4) as u8;
                Ok(Frame::Beacon(Beacon {
                    sender,
                    grid: (grid != 0x7FFF).then_some(grid),
                    group_tags,
                    status,
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block0_roundtrip() {
        let b = Block0 {
            sender: 123_456,
            dest: Destination::Group(0x0ABCDEF),
            msg_id: 0xBEEF,
            total: 4,
            hop: 0,
            ack_req: true,
            raw_text: false,
            payload: (0..BLOCK0_PAYLOAD_BITS)
                .map(|i| (i % 3 == 0) as u8)
                .collect(),
        };
        let bytes = b.pack();
        assert_eq!(bytes.len(), 48);
        assert_eq!(Frame::parse(&bytes).unwrap(), Frame::Block0(b));
        let mut bad = bytes.clone();
        bad[5] ^= 1;
        assert_eq!(Frame::parse(&bad), Err(ProtoError::Crc));
    }

    #[test]
    fn continuation_roundtrip() {
        let c = Continuation {
            seq: 3,
            msg_id: 7,
            payload: (0..CONT_PAYLOAD_BITS).map(|i| (i % 5 == 1) as u8).collect(),
        };
        assert_eq!(Continuation::parse(&c.pack()).unwrap(), c);
    }

    #[test]
    fn ack_and_beacon_roundtrip() {
        let a = Ack {
            sender: 1,
            dest: 2,
            msg_id: 300,
            snr_db: -17,
        };
        assert_eq!(Frame::parse(&a.pack()).unwrap(), Frame::Ack(a));
        let a = Ack {
            sender: 1,
            dest: 2,
            msg_id: 300,
            snr_db: 25,
        };
        assert_eq!(Frame::parse(&a.pack()).unwrap(), Frame::Ack(a));
        let b = Beacon {
            sender: 99,
            grid: Some(12345),
            group_tags: [0xABC, 0],
            status: 1,
        };
        assert_eq!(Frame::parse(&b.pack()).unwrap(), Frame::Beacon(b));
        let b = Beacon {
            sender: 99,
            grid: None,
            group_tags: [0, 0],
            status: 0,
        };
        assert_eq!(Frame::parse(&b.pack()).unwrap(), Frame::Beacon(b));
    }
}

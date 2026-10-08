//! Message ⇄ blocks (SPEC §7.3, §7.4, §9.4).

use crate::error::ProtoError;
use crate::frame::{
    BLOCK0_PAYLOAD_BITS, Block0, CONT_PAYLOAD_BITS, Continuation, Destination, Frame, MAX_BLOCKS,
    payload_capacity,
};
use crate::text;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub sender: u32,
    pub dest: Destination,
    pub msg_id: u16,
    pub ack_req: bool,
    pub text: String,
}

impl Message {
    /// Encoded payload bits and whether the raw fallback was used.
    pub fn payload(&self) -> (Vec<u8>, bool) {
        let coded = text::encode(&self.text);
        let raw_limit = self.text.len() * 8;
        if coded.len() > raw_limit {
            (text::encode_raw(&self.text), true)
        } else {
            (coded, false)
        }
    }

    pub fn blocks_needed(&self) -> Result<usize, ProtoError> {
        let (bits, _) = self.payload();
        (1..=MAX_BLOCKS)
            .find(|&n| payload_capacity(n) >= bits.len())
            .ok_or(ProtoError::TooLong(
                bits.len(),
                payload_capacity(MAX_BLOCKS),
            ))
    }

    /// Split into 32-byte info blocks ready for the modem.
    pub fn to_blocks(&self) -> Result<Vec<Vec<u8>>, ProtoError> {
        let (mut bits, raw_text) = self.payload();
        let n = self.blocks_needed()?;
        bits.resize(payload_capacity(n), 0);
        let (head, rest) = bits.split_at(BLOCK0_PAYLOAD_BITS);
        let mut out = vec![
            Block0 {
                sender: self.sender,
                dest: self.dest,
                msg_id: self.msg_id,
                total: n as u8,
                hop: 0,
                ack_req: self.ack_req,
                raw_text,
                payload: head.to_vec(),
            }
            .pack(),
        ];
        for (i, chunk) in rest.chunks(CONT_PAYLOAD_BITS).enumerate() {
            out.push(
                Continuation {
                    seq: (i + 1) as u8,
                    msg_id: self.msg_id,
                    payload: chunk.to_vec(),
                }
                .pack(),
            );
        }
        Ok(out)
    }
}

/// Collects the blocks of one burst and yields the message when complete.
#[derive(Debug, Default)]
pub struct MessageAssembler {
    block0: Option<Block0>,
    conts: Vec<Option<Continuation>>,
}

impl MessageAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a decoded 32-byte block at position `index` of the burst.
    pub fn push(&mut self, index: usize, bytes: &[u8]) -> Result<(), ProtoError> {
        if index == 0 {
            match Frame::parse(bytes)? {
                Frame::Block0(b) => {
                    self.conts.resize(b.total as usize - 1, None);
                    self.block0 = Some(b);
                    Ok(())
                }
                _ => Err(ProtoError::Field("block 0 is not a message")),
            }
        } else {
            let c = Continuation::parse(bytes)?;
            if c.seq as usize != index {
                return Err(ProtoError::Field("continuation seq mismatch"));
            }
            if self.conts.len() < index {
                self.conts.resize(index, None);
            }
            self.conts[index - 1] = Some(c);
            Ok(())
        }
    }

    /// Total blocks announced by block 0, if seen.
    pub fn total(&self) -> Option<usize> {
        self.block0.as_ref().map(|b| b.total as usize)
    }

    pub fn is_complete(&self) -> bool {
        self.block0.is_some() && self.conts.iter().all(Option::is_some)
    }

    pub fn finish(&self) -> Result<Message, ProtoError> {
        let b0 = self.block0.as_ref().ok_or(ProtoError::MissingBlock(0))?;
        let mut bits = b0.payload.clone();
        for (i, c) in self.conts.iter().enumerate() {
            let c = c.as_ref().ok_or(ProtoError::MissingBlock(i + 1))?;
            if c.msg_id != b0.msg_id {
                return Err(ProtoError::Field("continuation msg_id mismatch"));
            }
            bits.extend_from_slice(&c.payload);
        }
        let text = if b0.raw_text {
            text::decode_raw(&bits)?
        } else {
            text::decode(&bits)?
        };
        Ok(Message {
            sender: b0.sender,
            dest: b0.dest,
            msg_id: b0.msg_id,
            ack_req: b0.ack_req,
            text,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(text: &str) -> usize {
        let m = Message {
            sender: 42,
            dest: Destination::Group(7),
            msg_id: 0x1234,
            ack_req: false,
            text: text.to_string(),
        };
        let blocks = m.to_blocks().unwrap();
        let mut asm = MessageAssembler::new();
        for (i, b) in blocks.iter().enumerate() {
            asm.push(i, b).unwrap();
        }
        assert!(asm.is_complete());
        assert_eq!(asm.finish().unwrap(), m);
        blocks.len()
    }

    #[test]
    fn one_block_message() {
        assert_eq!(roundtrip("QRV på 40m, kaffe klart"), 1);
        assert_eq!(roundtrip(""), 1);
    }

    #[test]
    fn multi_block_message() {
        let t = "Hej allihopa! Antennen är uppe igen efter stormen, 40 m dipol på 12 meters höjd. \
                 Hör er fint här i Göteborg trots QRN. Någon som kör 60 m i kväll? 73 de SM6WJM";
        let n = roundtrip(t);
        assert!((2..=4).contains(&n), "{n} blocks");
    }

    #[test]
    fn raw_fallback_for_cjk_text() {
        let t = "今日は良い天気ですね";
        let m = Message {
            sender: 1,
            dest: Destination::All,
            msg_id: 1,
            ack_req: false,
            text: t.into(),
        };
        let (_, raw) = m.payload();
        assert!(raw);
        roundtrip(t);
    }

    #[test]
    fn too_long_is_rejected() {
        // Pseudo-random letters do not compress; 600 of them exceed 8 blocks.
        let mut x = 12345u32;
        let t: String = (0..600)
            .map(|_| {
                x = x.wrapping_mul(1103515245).wrapping_add(12345);
                char::from(b'a' + ((x >> 16) % 26) as u8)
            })
            .collect();
        let m = Message {
            sender: 1,
            dest: Destination::All,
            msg_id: 1,
            ack_req: false,
            text: t,
        };
        assert!(matches!(m.to_blocks(), Err(ProtoError::TooLong(_, _))));
    }
}

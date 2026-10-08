//! Burst construction (SPEC §7.2, §11): preamble, then blocks of pilots and
//! hop-mapped, interleaved LDPC codewords.

use crate::error::ModemError;
use crate::frame_kind::FrameKind;
use crate::gfsk;
use crate::hop;
use crate::interleave::interleave;
use crate::ldpc::Ldpc;
use crate::params::{PILOT_SYMBOLS, check_lane};
use crate::preamble;
use crate::profile::Profile;
use crate::symbols::bits_to_values;

/// What goes on air: kind, pattern phase, and the info bytes of each block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Burst {
    pub kind: FrameKind,
    pub phase: u8,
    pub blocks: Vec<Vec<u8>>,
}

impl Burst {
    pub fn new(kind: FrameKind, phase: u8, blocks: Vec<Vec<u8>>) -> Result<Self, ModemError> {
        if blocks.is_empty() || blocks.len() > kind.max_blocks() {
            return Err(ModemError::BlockCount(blocks.len()));
        }
        for (index, b) in blocks.iter().enumerate() {
            if b.len() != kind.info_bytes() {
                return Err(ModemError::BlockSize {
                    index,
                    got: b.len(),
                    expected: kind.info_bytes(),
                });
            }
        }
        Ok(Self {
            kind,
            phase: phase % 16,
            blocks,
        })
    }

    /// Tone index sequence for the whole burst.
    pub fn tones(&self) -> Vec<u8> {
        let ldpc = Ldpc::new(self.kind);
        let (rows, cols) = self.kind.interleaver();
        let mut out: Vec<u8> = preamble::tones(self.kind, self.phase).to_vec();
        let mut m = 0usize;
        for block in &self.blocks {
            for _ in 0..PILOT_SYMBOLS {
                out.push(hop::pilot_tone(m, self.phase));
                m += 1;
            }
            let bits = interleave(&ldpc.encode_bits(block), rows, cols);
            for d in bits_to_values(&bits) {
                out.push(hop::map(d, m, self.phase));
                m += 1;
            }
        }
        out
    }

    pub fn symbols(&self) -> usize {
        crate::params::PREAMBLE_SYMBOLS + self.blocks.len() * self.kind.block_symbols()
    }

    pub fn airtime_s(&self, profile: Profile) -> f64 {
        self.symbols() as f64 * profile.symbol_s()
    }
}

pub struct Transmitter {
    pub fs: u32,
    pub amplitude: f32,
}

impl Transmitter {
    pub fn new(fs: u32) -> Self {
        Self { fs, amplitude: 0.5 }
    }

    pub fn render(
        &self,
        burst: &Burst,
        lane: usize,
        profile: Profile,
        freq_offset_hz: f64,
    ) -> Result<Vec<f32>, ModemError> {
        check_lane(lane)?;
        gfsk::synthesize(
            &burst.tones(),
            lane,
            profile,
            self.fs,
            self.amplitude,
            freq_offset_hz,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_lengths() {
        let b = Burst::new(FrameKind::Long, 3, vec![vec![1u8; 32]; 4]).unwrap();
        assert_eq!(b.tones().len(), 24 + 4 * 132);
        assert!((b.airtime_s(Profile::Fast) - 17.664).abs() < 1e-9);
        let s = Burst::new(FrameKind::Short, 0, vec![vec![0u8; 16]]).unwrap();
        assert_eq!(s.tones().len(), 24 + 68);
        assert!(Burst::new(FrameKind::Short, 0, vec![vec![0u8; 16]; 2]).is_err());
        assert!(Burst::new(FrameKind::Long, 0, vec![vec![0u8; 16]]).is_err());
    }
}

//! Burst construction: preamble, then blocks of 4 pilots and 128 hop-mapped
//! GF(64) coded symbols.

use std::sync::OnceLock;

use fika_nb::NbCode;

use crate::error::ModemError;
use crate::frame_kind::FrameKind;
use crate::gfsk;
use crate::hop;
use crate::params::{PILOT_SYMBOLS, PREAMBLE_SYMBOLS};
use crate::preamble;
use crate::profile::Profile;
use crate::symbols::bytes_to_symbols;

/// The one GF(64) code used by both frame kinds.
pub fn code() -> &'static NbCode {
    static CODE: OnceLock<NbCode> = OnceLock::new();
    CODE.get_or_init(NbCode::long)
}

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

    /// Coded symbol sequence of one block (before hopping).
    pub fn coded_block(block: &[u8]) -> Vec<u8> {
        code().encode(&bytes_to_symbols(block))
    }

    /// Tone index sequence for the whole burst.
    pub fn tones(&self) -> Vec<u8> {
        let mut out: Vec<u8> = preamble::tones(self.kind, self.phase).to_vec();
        let mut m = 0usize;
        for block in &self.blocks {
            for _ in 0..PILOT_SYMBOLS {
                out.push(hop::pilot_tone(m, self.phase));
                m += 1;
            }
            for d in Self::coded_block(block) {
                out.push(hop::map(d, m, self.phase));
                m += 1;
            }
        }
        out
    }

    pub fn symbols(&self) -> usize {
        PREAMBLE_SYMBOLS + self.blocks.len() * self.kind.block_symbols()
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
        profile: Profile,
        freq_offset_hz: f64,
    ) -> Result<Vec<f32>, ModemError> {
        gfsk::synthesize(
            &burst.tones(),
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
        let b = Burst::new(FrameKind::Long, 3, vec![vec![1u8; 48]; 2]).unwrap();
        assert_eq!(b.tones().len(), 24 + 2 * 132);
        assert!(b.tones().iter().all(|&t| t < 64));
        assert!((b.airtime_s(Profile::Fast) - 288.0 / 37.5).abs() < 1e-9);
        assert!(Burst::new(FrameKind::Short, 0, vec![vec![0u8; 48]; 2]).is_err());
        assert!(Burst::new(FrameKind::Long, 0, vec![vec![0u8; 32]]).is_err());
    }
}

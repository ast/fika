use crate::costas::{LONG, LONG_RESIDUE, SHORT, SHORT_RESIDUE, scaled};
use crate::params::PILOT_SYMBOLS;

/// Long frames carry messages (1..8 blocks); short frames carry ACKs and
/// beacons (one block). Both use the same GF(64) code, n = 128, k = 64:
/// 48 info bytes per block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameKind {
    Long,
    Short,
}

impl FrameKind {
    pub const ALL: [FrameKind; 2] = [FrameKind::Long, FrameKind::Short];

    /// GF(64) symbols per codeword.
    pub fn coded_symbols(self) -> usize {
        128
    }

    pub fn info_symbols(self) -> usize {
        64
    }

    pub fn info_bits(self) -> usize {
        self.info_symbols() * crate::params::BITS_PER_SYMBOL
    }

    pub fn info_bytes(self) -> usize {
        self.info_bits() / 8
    }

    pub fn data_symbols(self) -> usize {
        self.coded_symbols()
    }

    /// Symbols per block on air, pilots included: 132.
    pub fn block_symbols(self) -> usize {
        PILOT_SYMBOLS + self.data_symbols()
    }

    /// SYNC tone of preamble symbol `n`.
    pub fn sync_tone(self, n: usize) -> u8 {
        match self {
            FrameKind::Long => scaled(LONG[n], LONG_RESIDUE),
            FrameKind::Short => scaled(SHORT[n], SHORT_RESIDUE),
        }
    }

    pub fn max_blocks(self) -> usize {
        match self {
            FrameKind::Long => crate::params::MAX_BLOCKS,
            FrameKind::Short => 1,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            FrameKind::Long => "long",
            FrameKind::Short => "short",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(FrameKind::Long.block_symbols(), 132);
        assert_eq!(FrameKind::Long.info_bytes(), 48);
        assert_eq!(FrameKind::Long.info_bits(), 384);
    }
}

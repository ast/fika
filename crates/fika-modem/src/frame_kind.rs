use labrador_ldpc::LDPCCode;

use crate::params::PILOT_SYMBOLS;

/// Long frames carry messages on (512,256) blocks; short frames carry ACKs
/// and beacons on one (256,128) block (SPEC §7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameKind {
    Long,
    Short,
}

impl FrameKind {
    pub const ALL: [FrameKind; 2] = [FrameKind::Long, FrameKind::Short];

    pub fn code(self) -> LDPCCode {
        match self {
            FrameKind::Long => LDPCCode::TC512,
            FrameKind::Short => LDPCCode::TC256,
        }
    }

    pub fn info_bits(self) -> usize {
        self.code().k()
    }

    pub fn info_bytes(self) -> usize {
        self.info_bits() / 8
    }

    pub fn coded_bits(self) -> usize {
        self.code().n()
    }

    pub fn data_symbols(self) -> usize {
        self.coded_bits() / 4
    }

    /// Symbols per block on air, pilots included: 132 long, 68 short.
    pub fn block_symbols(self) -> usize {
        PILOT_SYMBOLS + self.data_symbols()
    }

    /// Interleaver geometry: rows × columns, rows × columns = coded bits.
    pub fn interleaver(self) -> (usize, usize) {
        match self {
            FrameKind::Long => (16, 32),
            FrameKind::Short => (16, 16),
        }
    }

    /// Which Costas sequence the SYNC part of the preamble uses.
    pub fn sync_sequence(self) -> &'static [u8; 16] {
        match self {
            FrameKind::Long => &crate::costas::LONG,
            FrameKind::Short => &crate::costas::SHORT,
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
    fn sizes_match_spec() {
        assert_eq!(FrameKind::Long.block_symbols(), 132);
        assert_eq!(FrameKind::Short.block_symbols(), 68);
        assert_eq!(FrameKind::Long.info_bytes(), 32);
        assert_eq!(FrameKind::Short.info_bytes(), 16);
        let (r, c) = FrameKind::Long.interleaver();
        assert_eq!(r * c, 512);
    }
}

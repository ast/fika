//! Fixed constants of the v2 waveform: one band, 64 tones.

use crate::error::ModemError;

/// Tone spacing; every tone sits on this grid.
pub const BIN_HZ: f64 = 37.5;
/// Tones in the band.
pub const TONES: usize = 64;
/// Coded bits per symbol (GF(64) symbol).
pub const BITS_PER_SYMBOL: usize = 6;
/// Tone 0. The comb 318.75 + 37.5·k, k = 0..63, is centred on 1500 Hz and
/// spans 300–2700 Hz exactly (±half a tone).
pub const TONE0_HZ: f64 = 318.75;
/// Symbols in the preamble: 16 SYNC + 8 PHASE.
pub const PREAMBLE_SYMBOLS: usize = 24;
pub const SYNC_SYMBOLS: usize = 16;
pub const PHASE_SYMBOLS: usize = 8;
/// Known symbols at the start of every block.
pub const PILOT_SYMBOLS: usize = 4;
/// Maximum blocks in a long frame.
pub const MAX_BLOCKS: usize = 8;
/// The Gaussian pulse is defined in units of one fast symbol (26.67 ms).
pub const SHAPING_UNIT_S: f64 = 1.0 / BIN_HZ;
/// Gaussian pulse bandwidth-time product.
pub const GAUSS_BT: f64 = 2.0;
/// Sample rate the receiver runs at: 12000 / 37.5 = 320 samples per unit.
pub const RX_SAMPLE_RATE: u32 = 12_000;
/// Pattern phases a burst can start at (PHASE block alphabet).
pub const PHASES: usize = 16;

/// Frequency of tone `tone` (0..64).
pub fn tone_hz(tone: usize) -> f64 {
    debug_assert!(tone < TONES);
    TONE0_HZ + BIN_HZ * tone as f64
}

/// Samples per shaping unit (one fast symbol) at `fs`.
pub fn samples_per_unit(fs: u32) -> Result<usize, ModemError> {
    let n = fs as f64 * SHAPING_UNIT_S;
    if (n - n.round()).abs() > 1e-9 {
        return Err(ModemError::InvalidSampleRate(fs));
    }
    Ok(n.round() as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comb_fills_the_passband_exactly() {
        assert_eq!(tone_hz(0), 318.75);
        assert_eq!(tone_hz(63), 2681.25);
        assert_eq!(tone_hz(0) - BIN_HZ / 2.0, 300.0);
        assert_eq!(tone_hz(63) + BIN_HZ / 2.0, 2700.0);
    }

    #[test]
    fn unit_samples() {
        assert_eq!(samples_per_unit(12_000).unwrap(), 320);
        assert_eq!(samples_per_unit(48_000).unwrap(), 1280);
        assert!(samples_per_unit(32_000).is_err());
    }
}

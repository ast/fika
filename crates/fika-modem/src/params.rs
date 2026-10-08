//! Fixed constants of the waveform (SPEC §3, §4, §6).

use crate::error::ModemError;

/// Tone spacing and the frequency grid every tone sits on.
pub const BIN_HZ: f64 = 31.25;
/// Tones per lane.
pub const TONES: usize = 16;
/// Coded bits carried by one symbol.
pub const BITS_PER_SYMBOL: usize = 4;
/// Lanes in a 300–2700 Hz passband.
pub const LANES: usize = 4;
/// Distance between tone 0 of adjacent lanes: 16 tone bins + 2 guard bins.
pub const LANE_PITCH_HZ: f64 = 562.5;
/// Tone 0 of lane 0.
pub const LANE0_TONE0_HZ: f64 = 437.5;
/// Symbols in the preamble: 16 SYNC + 8 PHASE.
pub const PREAMBLE_SYMBOLS: usize = 24;
pub const SYNC_SYMBOLS: usize = 16;
pub const PHASE_SYMBOLS: usize = 8;
/// Known symbols at the start of every block.
pub const PILOT_SYMBOLS: usize = 4;
/// Maximum blocks in a long frame.
pub const MAX_BLOCKS: usize = 8;
/// The Gaussian pulse is defined in units of this duration for both profiles.
pub const SHAPING_UNIT_S: f64 = 0.032;
/// Gaussian pulse bandwidth-time product.
pub const GAUSS_BT: f64 = 2.0;
/// Sample rate the receiver runs at.
pub const RX_SAMPLE_RATE: u32 = 12_000;

/// Frequency of tone `tone` (0..16) in lane `lane` (0..4), SPEC §3.1.
pub fn tone_hz(lane: usize, tone: usize) -> f64 {
    debug_assert!(lane < LANES);
    debug_assert!(tone < TONES);
    LANE0_TONE0_HZ + LANE_PITCH_HZ * lane as f64 + BIN_HZ * tone as f64
}

/// Centre of a lane's occupied span.
pub fn lane_center_hz(lane: usize) -> f64 {
    tone_hz(lane, 0) + BIN_HZ * 7.5
}

pub fn check_lane(lane: usize) -> Result<(), ModemError> {
    if lane < LANES {
        Ok(())
    } else {
        Err(ModemError::InvalidLane(lane))
    }
}

/// Samples per 31.25 Hz bin period, i.e. per 32 ms, at `fs`.
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
    fn tone_plan_matches_spec_table() {
        assert_eq!(tone_hz(0, 0), 437.5);
        assert_eq!(tone_hz(0, 15), 906.25);
        assert_eq!(tone_hz(1, 0), 1000.0);
        assert_eq!(tone_hz(3, 15), 2593.75);
        assert_eq!(lane_center_hz(1), 1234.375);
        // Occupied span stays inside 300..2700 with margin.
        assert!(tone_hz(0, 0) - BIN_HZ / 2.0 > 300.0 + 100.0);
        assert!(tone_hz(3, 15) + BIN_HZ / 2.0 < 2700.0 - 90.0);
    }

    #[test]
    fn unit_samples() {
        assert_eq!(samples_per_unit(12_000).unwrap(), 384);
        assert_eq!(samples_per_unit(48_000).unwrap(), 1536);
        assert!(samples_per_unit(44_100).is_err());
    }
}

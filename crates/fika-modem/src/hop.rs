//! Per-symbol tone permutation and pilots on the 64-tone band.

use crate::costas::{H64, PHASE_RESIDUE, PILOT, scaled};
use crate::params::{PHASES, TONES};

/// Hop offset for data symbol `m` (counted from the first symbol after the
/// preamble, pilots included) at pattern phase `phase` (0..16).
#[inline]
pub fn offset(m: usize, phase: u8) -> u8 {
    H64[(m + 4 * phase as usize) % TONES]
}

#[inline]
pub fn map(d: u8, m: usize, phase: u8) -> u8 {
    ((d as usize + offset(m, phase) as usize) % TONES) as u8
}

#[inline]
pub fn unmap(tone: u8, m: usize, phase: u8) -> u8 {
    ((tone as usize + TONES - offset(m, phase) as usize) % TONES) as u8
}

/// Tone of pilot symbol `m` at `phase`: the PILOT sequence on residue 1.
#[inline]
pub fn pilot_tone(m: usize, phase: u8) -> u8 {
    scaled(PILOT[(m + phase as usize) % PHASES], PHASE_RESIDUE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_unmap_roundtrip() {
        for phase in 0..16u8 {
            for m in 0..200 {
                for d in 0..64u8 {
                    assert_eq!(unmap(map(d, m, phase), m, phase), d);
                }
            }
        }
    }
}

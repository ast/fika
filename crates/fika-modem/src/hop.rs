//! Per-symbol tone permutation (SPEC §5.2, §5.3).

use crate::costas::{LONG, PILOT};
use crate::params::TONES;

/// Tone of pilot symbol `m` (hop counter) at pattern phase `phase`.
#[inline]
pub fn pilot_tone(m: usize, phase: u8) -> u8 {
    PILOT[(m + phase as usize) % TONES]
}

/// Hop offset for data symbol `m` (counted from the first symbol after the
/// preamble, pilots included) at pattern phase `phase`.
#[inline]
pub fn offset(m: usize, phase: u8) -> u8 {
    LONG[(m + phase as usize) % TONES]
}

/// Tone transmitted for data value `d` at symbol `m`.
#[inline]
pub fn map(d: u8, m: usize, phase: u8) -> u8 {
    ((d as usize + offset(m, phase) as usize) % TONES) as u8
}

/// Data value carried by `tone` at symbol `m`.
#[inline]
pub fn unmap(tone: u8, m: usize, phase: u8) -> u8 {
    ((tone as usize + TONES - offset(m, phase) as usize) % TONES) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_unmap_roundtrip() {
        for phase in 0..16u8 {
            for m in 0..40 {
                for d in 0..16u8 {
                    assert_eq!(unmap(map(d, m, phase), m, phase), d);
                }
            }
        }
    }

    #[test]
    fn pilots_use_the_pilot_sequence() {
        assert_eq!(pilot_tone(5, 3), PILOT[8]);
    }
}

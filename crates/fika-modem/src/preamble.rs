//! Preamble construction (SPEC §6.1): 16 SYNC symbols from the frame kind's
//! Costas sequence, then 8 PHASE symbols from the PILOT sequence shifted by
//! the pattern phase.

use crate::costas::PILOT;
use crate::frame_kind::FrameKind;
use crate::params::{PHASE_SYMBOLS, PREAMBLE_SYMBOLS, SYNC_SYMBOLS, TONES};

pub fn tones(kind: FrameKind, phase: u8) -> [u8; PREAMBLE_SYMBOLS] {
    let mut out = [0u8; PREAMBLE_SYMBOLS];
    out[..SYNC_SYMBOLS].copy_from_slice(kind.sync_sequence());
    for n in 0..PHASE_SYMBOLS {
        out[SYNC_SYMBOLS + n] = PILOT[(n + phase as usize) % TONES];
    }
    out
}

/// Tone of PHASE symbol `n` for a given phase, used by the detector.
#[inline]
pub fn phase_tone(n: usize, phase: u8) -> u8 {
    PILOT[(n + phase as usize) % TONES]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_differ_in_at_least_seven_of_eight() {
        for a in 0..16u8 {
            for b in a + 1..16u8 {
                let same = (0..PHASE_SYMBOLS)
                    .filter(|&n| phase_tone(n, a) == phase_tone(n, b))
                    .count();
                assert!(same <= 1, "phases {a} {b} share {same}");
            }
        }
    }

    #[test]
    fn sync_is_phase_independent() {
        let a = tones(FrameKind::Long, 0);
        let b = tones(FrameKind::Long, 9);
        assert_eq!(a[..16], b[..16]);
        assert_ne!(a[16..], b[16..]);
        assert_eq!(tones(FrameKind::Short, 0)[..16], crate::costas::SHORT);
    }
}

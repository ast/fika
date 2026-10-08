//! Preamble: 16 SYNC symbols (long on tones 4k, short on 4k+2), then 8
//! PHASE symbols from the PILOT sequence shifted by the pattern phase on
//! tones 4k+1.

use crate::costas::{PHASE_RESIDUE, PILOT, scaled};
use crate::frame_kind::FrameKind;
use crate::params::{PHASE_SYMBOLS, PHASES, PREAMBLE_SYMBOLS, SYNC_SYMBOLS};

pub fn tones(kind: FrameKind, phase: u8) -> [u8; PREAMBLE_SYMBOLS] {
    let mut out = [0u8; PREAMBLE_SYMBOLS];
    for (n, slot) in out[..SYNC_SYMBOLS].iter_mut().enumerate() {
        *slot = kind.sync_tone(n);
    }
    for n in 0..PHASE_SYMBOLS {
        out[SYNC_SYMBOLS + n] = phase_tone(n, phase);
    }
    out
}

/// Tone of PHASE symbol `n` for a given phase.
#[inline]
pub fn phase_tone(n: usize, phase: u8) -> u8 {
    scaled(PILOT[(n + phase as usize) % PHASES], PHASE_RESIDUE)
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
                assert!(same <= 1);
            }
        }
    }

    #[test]
    fn sync_depends_on_kind_only() {
        let a = tones(FrameKind::Long, 0);
        let b = tones(FrameKind::Long, 9);
        assert_eq!(a[..16], b[..16]);
        assert_ne!(a[16..], b[16..]);
        assert_ne!(tones(FrameKind::Short, 0)[..16], a[..16]);
        assert!(a.iter().all(|&t| t < 64));
    }
}

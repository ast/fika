//! Costas sequences (SPEC §5.1). Three Welch arrays of order 16 from
//! p = 17 with different primitive roots, chosen so that no pair has more
//! than 4 coincidences under any time and frequency shift, including all
//! cyclic shifts (cyclic shifts of a Welch array are Welch arrays too, so
//! the earlier "flip" 15 − C was just C shifted by 8 and useless as a
//! marker).
//!
//! * `LONG`  (g = 3): SYNC of long frames and the data hop pattern.
//! * `SHORT` (g = 6): SYNC of short frames.
//! * `PILOT` (g = 7): PHASE block of the preamble and the block pilots.

use crate::params::TONES;

pub const LONG: [u8; TONES] = [0, 2, 8, 9, 12, 4, 14, 10, 15, 13, 7, 6, 3, 11, 1, 5];
pub const SHORT: [u8; TONES] = [0, 5, 1, 11, 3, 6, 7, 13, 15, 10, 14, 4, 12, 9, 8, 2];
pub const PILOT: [u8; TONES] = [0, 6, 14, 2, 3, 10, 8, 11, 15, 9, 1, 13, 12, 5, 7, 4];

/// Welch construction: a_i = g^i mod p − 1 for i in 0..p−1.
pub fn welch(p: u32, g: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(p as usize - 1);
    let mut x = 1u32;
    for _ in 0..p - 1 {
        out.push((x - 1) as u8);
        x = (x * g) % p;
    }
    out
}

/// True if `seq` is a permutation with the Costas property: every pair of
/// (time, frequency) difference vectors is distinct.
pub fn is_costas(seq: &[u8]) -> bool {
    let n = seq.len();
    let mut seen = vec![false; n];
    for &v in seq {
        if (v as usize) >= n || seen[v as usize] {
            return false;
        }
        seen[v as usize] = true;
    }
    let mut vectors = std::collections::HashSet::new();
    for i in 0..n {
        for j in i + 1..n {
            let dt = (j - i) as i32;
            let df = seq[j] as i32 - seq[i] as i32;
            if !vectors.insert((dt, df)) {
                return false;
            }
        }
    }
    true
}

/// Number of coincidences between `a` and `b` shifted by `dt` symbols and
/// `df` tones (aperiodic cross-correlation at one lag).
pub fn coincidences(a: &[u8], b: &[u8], dt: i32, df: i32) -> usize {
    let n = a.len() as i32;
    (0..n)
        .filter(|&i| {
            let j = i + dt;
            j >= 0 && j < n && b[j as usize] as i32 == a[i as usize] as i32 + df
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_is_welch_17_3() {
        assert_eq!(welch(17, 3), LONG.to_vec());
    }

    #[test]
    fn short_and_pilot_are_welch_6_and_7() {
        assert_eq!(welch(17, 6), SHORT.to_vec());
        assert_eq!(welch(17, 7), PILOT.to_vec());
    }

    #[test]
    fn all_three_are_costas() {
        assert!(is_costas(&LONG));
        assert!(is_costas(&SHORT));
        assert!(is_costas(&PILOT));
    }

    #[test]
    fn cross_coincidences_bounded_including_cyclic_shifts() {
        let seqs = [LONG, SHORT, PILOT];
        for a in 0..3 {
            for b in 0..3 {
                if a == b {
                    continue;
                }
                for k in 0..TONES {
                    let shifted: Vec<u8> = (0..TONES).map(|n| seqs[a][(n + k) % TONES]).collect();
                    for dt in -15..=15i32 {
                        for df in -15..=15i32 {
                            let c = coincidences(&shifted, &seqs[b], dt, df);
                            assert!(c <= 4, "seq {a} shift {k} vs {b}: {c} at ({dt},{df})");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn every_cyclic_shift_is_costas_and_shifts_agree_in_at_most_one_place() {
        let shift = |s: usize| -> Vec<u8> { (0..TONES).map(|n| LONG[(n + s) % TONES]).collect() };
        for s in 0..TONES {
            assert!(is_costas(&shift(s)), "shift {s}");
            for t in s + 1..TONES {
                let a = shift(s);
                let b = shift(t);
                let agree = (0..TONES).filter(|&n| a[n] == b[n]).count();
                assert!(agree <= 1, "shifts {s} and {t} agree in {agree} places");
            }
        }
    }

    #[test]
    fn autocorrelation_sidelobes_at_most_one() {
        for dt in -15..=15i32 {
            for df in -15..=15i32 {
                if dt == 0 && df == 0 {
                    continue;
                }
                assert!(coincidences(&LONG, &LONG, dt, df) <= 1);
            }
        }
    }
}

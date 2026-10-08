//! Costas sequences.
//!
//! Three Welch arrays of order 16 (p = 17) mark the preamble parts; they are
//! laid onto the 64-tone band scaled by 4 with a residue that keeps the sets
//! disjoint: long SYNC on tones 4k, short SYNC on 4k+2, PHASE and pilots on
//! 4k+1. The data hop pattern is an order-64 Costas array from the Welch
//! construction over p = 67 with the two lowest values dropped.

use crate::params::TONES;

pub const LONG: [u8; 16] = [0, 2, 8, 9, 12, 4, 14, 10, 15, 13, 7, 6, 3, 11, 1, 5];
pub const SHORT: [u8; 16] = [0, 5, 1, 11, 3, 6, 7, 13, 15, 10, 14, 4, 12, 9, 8, 2];
pub const PILOT: [u8; 16] = [0, 6, 14, 2, 3, 10, 8, 11, 15, 9, 1, 13, 12, 5, 7, 4];

/// Tone residues of the three roles on the 4-tone grid.
pub const LONG_RESIDUE: u8 = 0;
pub const PHASE_RESIDUE: u8 = 1;
pub const SHORT_RESIDUE: u8 = 2;

/// Order-64 Costas array for the data hop: y_i = 4·2^i mod 67, keep y ≥ 3,
/// x = y − 3 (a Welch array with its two lowest rows removed stays Costas).
pub const H64: [u8; TONES] = [
    1, 5, 13, 29, 61, 58, 52, 40, 16, 35, 6, 15, 33, 2, 7, 17, 37, 10, 23, 49, 34, 4, 11, 25, 53,
    42, 20, 43, 22, 47, 30, 63, 62, 60, 56, 48, 32, 0, 3, 9, 21, 45, 26, 55, 46, 28, 59, 54, 44,
    24, 51, 38, 12, 27, 57, 50, 36, 8, 19, 41, 18, 39, 14, 31,
];

/// Tone of an order-16 sequence element placed on the 64-tone grid.
#[inline]
pub fn scaled(value: u8, residue: u8) -> u8 {
    4 * value + residue
}

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
    fn order16_arrays_are_welch_and_costas() {
        assert_eq!(welch(17, 3), LONG.to_vec());
        assert_eq!(welch(17, 6), SHORT.to_vec());
        assert_eq!(welch(17, 7), PILOT.to_vec());
        for s in [LONG, SHORT, PILOT] {
            assert!(is_costas(&s));
        }
    }

    #[test]
    fn h64_is_the_truncated_welch_67_array_and_costas() {
        let mut y = 4u32;
        let mut built = Vec::new();
        for _ in 0..66 {
            if y >= 3 {
                built.push((y - 3) as u8);
            }
            y = (y * 2) % 67;
        }
        assert_eq!(built, H64.to_vec());
        assert!(is_costas(&H64));
    }

    #[test]
    fn scaled_roles_are_disjoint() {
        let long: Vec<u8> = LONG.iter().map(|&v| scaled(v, LONG_RESIDUE)).collect();
        let short: Vec<u8> = SHORT.iter().map(|&v| scaled(v, SHORT_RESIDUE)).collect();
        let phase: Vec<u8> = PILOT.iter().map(|&v| scaled(v, PHASE_RESIDUE)).collect();
        assert!(
            long.iter()
                .all(|t| !short.contains(t) && !phase.contains(t))
        );
        assert!(short.iter().all(|t| !phase.contains(t)));
        assert!(
            long.iter()
                .chain(short.iter())
                .chain(phase.iter())
                .all(|&t| (t as usize) < TONES)
        );
        // Scaling preserves the Costas property.
        assert!(is_costas(&LONG));
    }
}

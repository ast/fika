//! 28-bit callsign packing (SPEC §9.1). Standard callsigns use the FT8
//! six-character index without FT8's token offset; others are hashed.

use crate::error::ProtoError;

const A1: &[u8] = b" 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const A2: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const A3: &[u8] = b"0123456789";
const A4: &[u8] = b" ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// First value that is not a standard callsign.
pub const HASH_BASE: u32 = 37 * 36 * 10 * 27 * 27 * 27; // 262_177_560
pub const HASH_BITS: u32 = 22;
pub const MAX_PACKED: u32 = HASH_BASE + (1 << HASH_BITS) - 1;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Callsign {
    Standard(String),
    Hashed(u32),
}

impl std::fmt::Display for Callsign {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Callsign::Standard(s) => f.write_str(s),
            Callsign::Hashed(h) => write!(f, "<{h:06X}>"),
        }
    }
}

pub fn fnv1a32(bytes: &[u8]) -> u32 {
    let mut h = 0x811C9DC5u32;
    for &b in bytes {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    h
}

/// Normalise to the six-character FT8 form, or None if not standard.
fn six_char(call: &str) -> Option<[u8; 6]> {
    let up = call.trim().to_ascii_uppercase();
    let b = up.as_bytes();
    if b.len() < 3 || b.len() > 6 {
        return None;
    }
    // Digit must be at index 2, or at index 1 (then prepend a space).
    let padded: Vec<u8> = if b[2].is_ascii_digit() {
        b.to_vec()
    } else if b[1].is_ascii_digit() && b.len() <= 5 {
        let mut v = vec![b' '];
        v.extend_from_slice(b);
        v
    } else {
        return None;
    };
    let mut out = [b' '; 6];
    out[..padded.len()].copy_from_slice(&padded);
    let ok = A1.contains(&out[0])
        && A2.contains(&out[1])
        && A3.contains(&out[2])
        && A4.contains(&out[3])
        && A4.contains(&out[4])
        && A4.contains(&out[5]);
    // Suffix letters must be contiguous (no inner spaces).
    let suffix = &out[3..];
    let trimmed_len = suffix.iter().rposition(|&c| c != b' ').map_or(0, |p| p + 1);
    if suffix[..trimmed_len].contains(&b' ') || trimmed_len == 0 {
        return None;
    }
    ok.then_some(out)
}

pub fn pack(call: &str) -> u32 {
    match six_char(call) {
        Some(c) => {
            let idx = |set: &[u8], ch: u8| set.iter().position(|&x| x == ch).unwrap() as u32;
            let mut n = idx(A1, c[0]);
            n = n * 36 + idx(A2, c[1]);
            n = n * 10 + idx(A3, c[2]);
            n = n * 27 + idx(A4, c[3]);
            n = n * 27 + idx(A4, c[4]);
            n = n * 27 + idx(A4, c[5]);
            n
        }
        None => {
            HASH_BASE
                + (fnv1a32(call.trim().to_ascii_uppercase().as_bytes()) & ((1 << HASH_BITS) - 1))
        }
    }
}

pub fn unpack(mut n: u32) -> Result<Callsign, ProtoError> {
    if n >= HASH_BASE {
        if n > MAX_PACKED {
            return Err(ProtoError::Field("callsign value out of range"));
        }
        return Ok(Callsign::Hashed(n - HASH_BASE));
    }
    let mut c = [b' '; 6];
    c[5] = A4[(n % 27) as usize];
    n /= 27;
    c[4] = A4[(n % 27) as usize];
    n /= 27;
    c[3] = A4[(n % 27) as usize];
    n /= 27;
    c[2] = A3[(n % 10) as usize];
    n /= 10;
    c[1] = A2[(n % 36) as usize];
    n /= 36;
    c[0] = A1[n as usize];
    let s = String::from_utf8_lossy(&c).trim().to_string();
    Ok(Callsign::Standard(s))
}

impl Callsign {
    pub fn parse(call: &str) -> Self {
        unpack(pack(call)).expect("pack always yields a valid value")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_roundtrip() {
        for call in [
            "SM6WJM", "AD8KM", "K1ABC", "W1AW", "SA6BSS", "9A1A", "VK2ABC", "G4X",
        ] {
            let n = pack(call);
            assert!(n < HASH_BASE, "{call}");
            assert_eq!(
                unpack(n).unwrap(),
                Callsign::Standard(call.to_string()),
                "{call}"
            );
        }
    }

    #[test]
    fn nonstandard_is_hashed() {
        let n = pack("SM6WJM/P");
        assert!((HASH_BASE..=MAX_PACKED).contains(&n));
        assert!(matches!(unpack(n).unwrap(), Callsign::Hashed(_)));
        assert_eq!(pack("sm6wjm/p"), pack("SM6WJM/P"));
    }

    #[test]
    fn max_value_fits_28_bits() {
        const _: () = assert!(MAX_PACKED < (1 << 28));
        assert_eq!(HASH_BASE, 262_177_560);
    }
}

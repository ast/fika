//! Bit and symbol packing helpers. Bits are `u8` values 0/1, MSB first.

use crate::params::BITS_PER_SYMBOL;

pub fn bytes_to_bits(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .flat_map(|&b| (0..8).rev().map(move |i| (b >> i) & 1))
        .collect()
}

pub fn bits_to_bytes(bits: &[u8]) -> Vec<u8> {
    bits.chunks(8)
        .map(|c| c.iter().fold(0u8, |acc, &b| (acc << 1) | (b & 1)) << (8 - c.len()))
        .collect()
}

/// Group coded bits into 4-bit data values, MSB first.
pub fn bits_to_values(bits: &[u8]) -> Vec<u8> {
    debug_assert_eq!(bits.len() % BITS_PER_SYMBOL, 0);
    bits.chunks(BITS_PER_SYMBOL)
        .map(|c| c.iter().fold(0u8, |acc, &b| (acc << 1) | (b & 1)))
        .collect()
}

/// Bit `b` (0 = MSB) of data value `d`.
#[inline]
pub fn value_bit(d: u8, b: usize) -> u8 {
    (d >> (BITS_PER_SYMBOL - 1 - b)) & 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let bytes = [0xA5u8, 0x3C, 0xFF, 0x00];
        let bits = bytes_to_bits(&bytes);
        assert_eq!(bits.len(), 32);
        assert_eq!(bits_to_bytes(&bits), bytes);
        let vals = bits_to_values(&bits);
        assert_eq!(vals, vec![0xA, 0x5, 0x3, 0xC, 0xF, 0xF, 0x0, 0x0]);
        assert_eq!(value_bit(0b1000, 0), 1);
        assert_eq!(value_bit(0b0001, 3), 1);
    }
}

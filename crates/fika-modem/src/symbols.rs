//! Bit, byte and GF(64) symbol packing. Bits are `u8` 0/1, MSB first.

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

/// 48 bytes → 64 six-bit symbols (MSB first).
pub fn bytes_to_symbols(bytes: &[u8]) -> Vec<u8> {
    let bits = bytes_to_bits(bytes);
    assert_eq!(bits.len() % 6, 0, "byte count must be a multiple of 3");
    bits.chunks(6)
        .map(|c| c.iter().fold(0u8, |acc, &b| (acc << 1) | b))
        .collect()
}

/// 64 six-bit symbols → 48 bytes.
pub fn symbols_to_bytes(symbols: &[u8]) -> Vec<u8> {
    let bits: Vec<u8> = symbols
        .iter()
        .flat_map(|&s| (0..6).rev().map(move |i| (s >> i) & 1))
        .collect();
    bits_to_bytes(&bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let bytes: Vec<u8> = (0..48u8).map(|i| i.wrapping_mul(53) ^ 0x3C).collect();
        let syms = bytes_to_symbols(&bytes);
        assert_eq!(syms.len(), 64);
        assert!(syms.iter().all(|&s| s < 64));
        assert_eq!(symbols_to_bytes(&syms), bytes);
        assert_eq!(bits_to_bytes(&bytes_to_bits(&bytes)), bytes);
    }
}

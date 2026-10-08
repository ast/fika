//! MSB-first bit packing over byte buffers.

pub struct BitWriter {
    bits: Vec<u8>,
}

impl Default for BitWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl BitWriter {
    pub fn new() -> Self {
        Self { bits: Vec::new() }
    }

    pub fn push(&mut self, value: u64, width: usize) {
        debug_assert!(width <= 64);
        for i in (0..width).rev() {
            self.bits.push(((value >> i) & 1) as u8);
        }
    }

    pub fn push_bits(&mut self, bits: &[u8]) {
        self.bits.extend_from_slice(bits);
    }

    pub fn len(&self) -> usize {
        self.bits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bits.is_empty()
    }

    pub fn bits(&self) -> &[u8] {
        &self.bits
    }

    pub fn into_bits(self) -> Vec<u8> {
        self.bits
    }

    pub fn pad_to(&mut self, len: usize) {
        while self.bits.len() < len {
            self.bits.push(0);
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        fika_modem::symbols::bits_to_bytes(&self.bits)
    }
}

pub struct BitReader<'a> {
    bits: &'a [u8],
    pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(bits: &'a [u8]) -> Self {
        Self { bits, pos: 0 }
    }

    /// Read `width` bits; bits past the end read as zero.
    pub fn read(&mut self, width: usize) -> u64 {
        let mut v = 0u64;
        for _ in 0..width {
            let b = self.bits.get(self.pos).copied().unwrap_or(0);
            v = (v << 1) | b as u64;
            self.pos += 1;
        }
        v
    }

    pub fn read_bits(&mut self, width: usize) -> &'a [u8] {
        let end = (self.pos + width).min(self.bits.len());
        let s = &self.bits[self.pos.min(self.bits.len())..end];
        self.pos += width;
        s
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.bits.len().saturating_sub(self.pos)
    }
}

pub fn bytes_to_bits(bytes: &[u8]) -> Vec<u8> {
    fika_modem::symbols::bytes_to_bits(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut w = BitWriter::new();
        w.push(0b101, 3);
        w.push(0xABCD, 16);
        w.push(1, 1);
        w.pad_to(24);
        let bytes = w.to_bytes();
        assert_eq!(bytes.len(), 3);
        let bits = bytes_to_bits(&bytes);
        let mut r = BitReader::new(&bits);
        assert_eq!(r.read(3), 0b101);
        assert_eq!(r.read(16), 0xABCD);
        assert_eq!(r.read(1), 1);
        assert_eq!(r.read(4), 0);
        assert_eq!(r.read(10), 0); // past the end
    }
}

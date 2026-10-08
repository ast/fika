//! GF(64) = GF(2)[x] / (x^6 + x + 1), log/antilog tables.

pub const Q: usize = 64;
pub const M: usize = 6;
const POLY: u16 = 0b100_0011; // x^6 + x + 1

pub struct Gf64 {
    exp: [u8; 128],
    log: [u8; 64],
}

impl Default for Gf64 {
    fn default() -> Self {
        Self::new()
    }
}

impl Gf64 {
    pub const fn new() -> Self {
        let mut exp = [0u8; 128];
        let mut log = [0u8; 64];
        let mut x: u16 = 1;
        let mut i = 0;
        while i < 63 {
            exp[i] = x as u8;
            log[x as usize] = i as u8;
            x <<= 1;
            if x & 0x40 != 0 {
                x ^= POLY;
            }
            i += 1;
        }
        // Wrap so exp[i + 63] = exp[i] for mult without a modulo.
        let mut j = 63;
        while j < 128 {
            exp[j] = exp[j - 63];
            j += 1;
        }
        Self { exp, log }
    }

    #[inline]
    pub fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            self.exp[self.log[a as usize] as usize + self.log[b as usize] as usize]
        }
    }

    #[inline]
    pub fn inv(&self, a: u8) -> u8 {
        debug_assert!(a != 0);
        self.exp[63 - self.log[a as usize] as usize]
    }

    #[inline]
    pub fn div(&self, a: u8, b: u8) -> u8 {
        self.mul(a, self.inv(b))
    }

    /// Element alpha^i.
    #[inline]
    pub fn alpha_pow(&self, i: usize) -> u8 {
        self.exp[i % 63]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_axioms() {
        let f = Gf64::new();
        // alpha generates all 63 nonzero elements.
        let mut seen = [false; 64];
        for i in 0..63 {
            let v = f.alpha_pow(i);
            assert!(!seen[v as usize]);
            seen[v as usize] = true;
        }
        for a in 1..64u8 {
            assert_eq!(f.mul(a, f.inv(a)), 1);
            for b in 1..64u8 {
                assert_eq!(f.mul(a, b), f.mul(b, a));
                assert_eq!(f.div(f.mul(a, b), b), a);
            }
        }
        // Distributivity on a sample.
        assert_eq!(f.mul(5, 7 ^ 9), f.mul(5, 7) ^ f.mul(5, 9));
    }
}
